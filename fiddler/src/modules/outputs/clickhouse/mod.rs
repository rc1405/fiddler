//! ClickHouse output module for batch data insertion.
//!
//! This module provides a batching output implementation that sends data
//! to a ClickHouse database using bulk INSERT operations.
//!
//! # Configuration
//!
//! ```yaml
//! output:
//!   clickhouse:
//!     url: "http://localhost:8123"        # Required: ClickHouse HTTP endpoint
//!     database: "default"                 # Optional: Database name (default: "default")
//!     table: "events"                     # Required: Table name
//!     username: "default"                 # Optional: Username for authentication
//!     password: ""                        # Optional: Password for authentication
//!     batch:                              # Optional: Batching policy
//!       size: 1000                        # Optional: Batch size (default: 500)
//!     create_table: false                 # Optional: Auto-create table (default: false)
//!     columns:                            # Optional: Column definitions for table creation
//!       - name: "timestamp"
//!         type: "DateTime64(3)"
//!       - name: "data"
//!         type: "String"
//! ```
//!
//! # Message Format
//!
//! Messages are expected to be valid JSON objects. Each JSON object is inserted
//! as a row in the target table. The JSON keys should match the column names
//! in the ClickHouse table.
//!
//! # Example
//!
//! Given a table:
//! ```sql
//! CREATE TABLE events (
//!     timestamp DateTime64(3),
//!     event_type String,
//!     data String
//! ) ENGINE = MergeTree() ORDER BY timestamp
//! ```
//!
//! And messages like:
//! ```json
//! {"timestamp": "2024-01-15 10:30:00.000", "event_type": "click", "data": "..."}
//! ```

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::{BatchingPolicy, Closer, Error, MessageBatch, OutputBatch};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, Sender};
use serde::Deserialize;
use serde_yaml::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, warn};

const DEFAULT_DATABASE: &str = "default";

/// Column definition for table creation.
#[derive(Debug, Deserialize, Clone)]
pub struct ColumnDef {
    /// Column name.
    pub name: String,
    /// ClickHouse data type (e.g., "String", "DateTime64(3)", "UInt64").
    #[serde(rename = "type")]
    pub col_type: String,
}

/// ClickHouse output configuration.
#[derive(Deserialize, Clone)]
pub struct ClickHouseOutputConfig {
    /// ClickHouse HTTP endpoint URL (required).
    pub url: String,
    /// Database name (default: "default").
    #[serde(default = "default_database")]
    pub database: String,
    /// Table name (required).
    pub table: String,
    /// Username for authentication.
    pub username: Option<String>,
    /// Password for authentication.
    pub password: Option<String>,
    /// Batching policy configuration.
    #[serde(default)]
    pub batch: Option<BatchingPolicy>,
    /// Whether to auto-create the table if it doesn't exist (default: false).
    #[serde(default)]
    pub create_table: bool,
    /// Column definitions for table creation.
    #[serde(default)]
    pub columns: Vec<ColumnDef>,
}

fn default_database() -> String {
    DEFAULT_DATABASE.to_string()
}

/// Validate a ClickHouse identifier (database, table, column name).
fn validate_identifier(identifier: &str) -> Result<(), Error> {
    if identifier.is_empty() {
        return Err(Error::Validation("Identifier cannot be empty".into()));
    }

    let first_char = identifier.chars().next().unwrap_or('0');
    if !first_char.is_alphabetic() && first_char != '_' {
        return Err(Error::Validation(format!(
            "Invalid identifier '{}': must start with letter or underscore",
            identifier
        )));
    }

    if !identifier.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(Error::Validation(format!(
            "Invalid identifier '{}': must contain only alphanumeric characters and underscores",
            identifier
        )));
    }

    Ok(())
}

/// Request sent to the background handler task.
struct Request {
    messages: MessageBatch,
    response: Sender<Result<(), Error>>,
}

/// ClickHouse HTTP client for the background handler.
struct ClickHouseClient {
    client: reqwest::Client,
    url: String,
    database: String,
    table: String,
    username: Option<String>,
    password: Option<String>,
    table_created: AtomicBool,
    create_table_enabled: bool,
    columns: Vec<ColumnDef>,
}

impl ClickHouseClient {
    fn new(config: &ClickHouseOutputConfig) -> Result<Self, Error> {
        // Validate identifiers
        validate_identifier(&config.database)?;
        validate_identifier(&config.table)?;
        for col in &config.columns {
            validate_identifier(&col.name)?;
        }

        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(2)
            .pool_idle_timeout(Duration::from_secs(90))
            .timeout(Duration::from_secs(60)) // Longer timeout for batch inserts
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| Error::ExecutionError(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self {
            client,
            url: config.url.clone(),
            database: config.database.clone(),
            table: config.table.clone(),
            username: config.username.clone(),
            password: config.password.clone(),
            table_created: AtomicBool::new(false),
            create_table_enabled: config.create_table,
            columns: config.columns.clone(),
        })
    }

    async fn execute_query(&self, query: &str) -> Result<(), Error> {
        let url = format!("{}/?database={}", self.url, self.database);

        let mut request = self.client.post(&url).body(query.to_string());

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        } else if let Some(username) = &self.username {
            request = request.basic_auth(username, None::<&str>);
        }

        let response = request
            .send()
            .await
            .map_err(|e| Error::OutputError(format!("ClickHouse request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::OutputError(format!(
                "ClickHouse query failed with status {}: {}",
                status, body
            )));
        }

        Ok(())
    }

    async fn create_table_if_not_exists(&self) -> Result<(), Error> {
        if self.columns.is_empty() {
            return Err(Error::ConfigFailedValidation(
                "columns must be specified when create_table is true".into(),
            ));
        }

        let columns_def: String = self
            .columns
            .iter()
            .map(|c| format!("    {} {}", c.name, c.col_type))
            .collect::<Vec<_>>()
            .join(",\n");

        // Default to MergeTree engine with first column as order key
        let order_key = &self.columns[0].name;

        let create_query = format!(
            r#"CREATE TABLE IF NOT EXISTS {}.{} (
{}
) ENGINE = MergeTree()
ORDER BY ({})"#,
            self.database, self.table, columns_def, order_key
        );

        self.execute_query(&create_query).await
    }

    async fn insert_batch(&self, messages: &MessageBatch) -> Result<(), Error> {
        if messages.is_empty() {
            return Ok(());
        }

        // Ensure table exists if configured
        if self.create_table_enabled && !self.table_created.load(Ordering::Relaxed) {
            if self.create_table_if_not_exists().await.is_ok() {
                self.table_created.store(true, Ordering::Relaxed);
            }
        }

        // Build JSONEachRow format for ClickHouse
        // Each line is a JSON object representing one row
        let mut json_rows = Vec::with_capacity(messages.len());

        for msg in messages {
            // Validate that message is valid JSON
            match serde_json::from_slice::<serde_json::Value>(&msg.bytes) {
                Ok(json) => {
                    if json.is_object() {
                        // Compact JSON without newlines for JSONEachRow format
                        json_rows.push(json.to_string());
                    } else {
                        warn!("Skipping non-object JSON message");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Skipping invalid JSON message");
                }
            }
        }

        if json_rows.is_empty() {
            return Ok(());
        }

        // Use JSONEachRow format which is efficient and flexible
        let url = format!(
            "{}/?database={}&query=INSERT%20INTO%20{}%20FORMAT%20JSONEachRow",
            self.url, self.database, self.table
        );

        let body = json_rows.join("\n");

        let mut request = self.client.post(&url).body(body);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        } else if let Some(username) = &self.username {
            request = request.basic_auth(username, None::<&str>);
        }

        let response = request
            .send()
            .await
            .map_err(|e| Error::OutputError(format!("ClickHouse insert failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::OutputError(format!(
                "ClickHouse insert failed with status {}: {}",
                status, body
            )));
        }

        debug!(count = json_rows.len(), "Inserted batch to ClickHouse");
        Ok(())
    }
}

/// Background handler task that processes insert requests.
async fn clickhouse_handler(
    client: Arc<ClickHouseClient>,
    requests: Receiver<Request>,
) -> Result<(), Error> {
    while let Ok(req) = requests.recv_async().await {
        let result = client.insert_batch(&req.messages).await;

        if let Err(ref e) = result {
            error!(error = %e, "Failed to insert batch to ClickHouse");
        }

        // Send result back to caller
        if req.response.send_async(result).await.is_err() {
            warn!("Response channel closed before result could be sent");
        }
    }

    debug!("ClickHouse handler task exiting");
    Ok(())
}

/// ClickHouse batch output.
///
/// Collects messages into batches and inserts them into ClickHouse
/// using the JSONEachRow format for efficient bulk loading.
pub struct ClickHouseOutput {
    sender: Sender<Request>,
    batch_size: usize,
    interval: Duration,
}

impl ClickHouseOutput {
    /// Creates a new ClickHouse output from configuration.
    pub fn new(config: ClickHouseOutputConfig) -> Result<Self, Error> {
        let client = Arc::new(ClickHouseClient::new(&config)?);

        // Extract batch settings
        let batch_size = config.batch.as_ref().map_or(500, |b| b.effective_size());
        let interval = config
            .batch
            .as_ref()
            .map_or(Duration::from_secs(10), |b| b.effective_duration());

        // Create channel for requests
        let (sender, receiver) = bounded(0);

        // Spawn background handler
        tokio::spawn(clickhouse_handler(client, receiver));

        debug!(
            url = %config.url,
            database = %config.database,
            table = %config.table,
            batch_size = batch_size,
            interval_secs = interval.as_secs(),
            "ClickHouse output initialized"
        );

        Ok(Self {
            sender,
            batch_size,
            interval,
        })
    }
}

#[async_trait]
impl OutputBatch for ClickHouseOutput {
    async fn write_batch(&mut self, messages: MessageBatch) -> Result<(), Error> {
        debug!(count = messages.len(), "Sending batch to ClickHouse");

        let (tx, rx) = bounded(0);
        self.sender
            .send_async(Request {
                messages,
                response: tx,
            })
            .await
            .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;

        // Wait for result
        rx.recv_async().await.map_err(|e| {
            Error::OutputError(format!("Failed to receive response from handler: {}", e))
        })??;

        Ok(())
    }

    async fn batch_size(&self) -> usize {
        self.batch_size
    }

    async fn interval(&self) -> Duration {
        self.interval
    }
}

#[async_trait]
impl Closer for ClickHouseOutput {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("ClickHouse output closing");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_clickhouse_output(conf: Value) -> Result<ExecutionType, Error> {
    let config: ClickHouseOutputConfig = serde_yaml::from_value(conf)?;

    // Validate required fields
    if config.url.is_empty() {
        return Err(Error::ConfigFailedValidation("url is required".into()));
    }
    if config.table.is_empty() {
        return Err(Error::ConfigFailedValidation("table is required".into()));
    }

    // Validate auth configuration
    if config.username.is_some() && config.password.is_none() {
        return Err(Error::ConfigFailedValidation(
            "password is required when username is specified".into(),
        ));
    }

    // Validate create_table requires columns
    if config.create_table && config.columns.is_empty() {
        return Err(Error::ConfigFailedValidation(
            "columns must be specified when create_table is true".into(),
        ));
    }

    Ok(ExecutionType::OutputBatch(Box::new(ClickHouseOutput::new(
        config,
    )?)))
}

/// Registers the ClickHouse output plugin.
pub(crate) fn register_clickhouse() -> Result<(), Error> {
    let config = r#"type: object
required:
  - url
  - table
properties:
  url:
    type: string
    description: "ClickHouse HTTP endpoint URL"
  database:
    type: string
    default: "default"
    description: "Database name"
  table:
    type: string
    description: "Table name"
  username:
    type: string
    description: "Username for authentication"
  password:
    type: string
    description: "Password for authentication"
  batch:
    type: object
    properties:
      size:
        type: integer
        description: "Batch size (default: 500)"
      duration:
        type: string
        description: "Flush interval (default: 10s)"
    description: "Batching policy configuration"
  create_table:
    type: boolean
    default: false
    description: "Auto-create table if not exists"
  columns:
    type: array
    items:
      type: object
      properties:
        name:
          type: string
        type:
          type: string
      required:
        - name
        - type
    description: "Column definitions for table creation"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "clickhouse".into(),
        ItemType::OutputBatch,
        conf_spec,
        create_clickhouse_output,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_identifier_valid() {
        assert!(validate_identifier("events").is_ok());
        assert!(validate_identifier("my_table").is_ok());
        assert!(validate_identifier("_private").is_ok());
        assert!(validate_identifier("Table123").is_ok());
    }

    #[test]
    fn test_validate_identifier_invalid() {
        assert!(validate_identifier("").is_err());
        assert!(validate_identifier("123invalid").is_err());
        assert!(validate_identifier("invalid-name").is_err());
        assert!(validate_identifier("invalid.name").is_err());
    }

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
url: "http://localhost:8123"
database: "mydb"
table: "events"
username: "user"
password: "pass"
batch:
  size: 1000
create_table: true
columns:
  - name: timestamp
    type: DateTime64(3)
  - name: data
    type: String
"#;
        let config: ClickHouseOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url, "http://localhost:8123");
        assert_eq!(config.database, "mydb");
        assert_eq!(config.table, "events");
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
        assert!(config.create_table);
        assert_eq!(config.columns.len(), 2);
        assert_eq!(config.columns[0].name, "timestamp");
        assert_eq!(config.columns[0].col_type, "DateTime64(3)");
        assert_eq!(config.batch.as_ref().unwrap().effective_size(), 1000);
    }

    #[test]
    fn test_config_deserialization_with_duration() {
        let yaml = r#"
url: "http://localhost:8123"
database: "mydb"
table: "events"
username: "user"
password: "pass"
batch:
  size: 1000
  duration: 500ms
create_table: true
columns:
  - name: timestamp
    type: DateTime64(3)
  - name: data
    type: String
"#;
        let config: ClickHouseOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url, "http://localhost:8123");
        assert_eq!(config.database, "mydb");
        assert_eq!(config.table, "events");
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
        assert!(config.create_table);
        assert_eq!(config.columns.len(), 2);
        assert_eq!(config.columns[0].name, "timestamp");
        assert_eq!(config.columns[0].col_type, "DateTime64(3)");
        assert_eq!(config.batch.as_ref().unwrap().effective_size(), 1000);
    }

    #[test]
    fn test_config_defaults() {
        let yaml = r#"
url: "http://localhost:8123"
table: "events"
"#;
        let config: ClickHouseOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.database, "default");
        assert!(config.username.is_none());
        assert!(config.password.is_none());
        assert!(!config.create_table);
        assert!(config.columns.is_empty());
    }

    #[test]
    fn test_register_clickhouse() {
        let result = register_clickhouse();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
