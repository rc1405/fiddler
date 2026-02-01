//! ClickHouse metrics backend.
//!
//! This module provides a metrics implementation that sends metrics
//! to a ClickHouse database.
//!
//! # Configuration
//!
//! ```yaml
//! metrics:
//!   clickhouse:
//!     url: "http://localhost:8123"        # Required: ClickHouse HTTP endpoint
//!     database: "fiddler"                 # Optional: Database name (default: "fiddler")
//!     table: "metrics"                    # Optional: Table name (default: "metrics")
//!     username: "default"                 # Optional: Username for authentication
//!     password: ""                        # Optional: Password for authentication
//!     include:                            # Optional: list of metrics to include (all if not set)
//!       - total_received
//!       - throughput_per_sec
//!     exclude:                            # Optional: list of metrics to exclude (none if not set)
//!       - stale_entries_removed
//!     dimensions:                         # Optional: additional dimensions for all metrics
//!       - name: "environment"
//!         value: "production"
//!     batch_size: 100                     # Optional: Number of metrics to batch (default: 100)
//!     flush_interval_ms: 5000             # Optional: Max time between flushes in ms (default: 5000)
//!     create_table: true                  # Optional: Auto-create table if not exists (default: true)
//!     ttl_days: 0                         # Optional: TTL for metrics in days (default: 0 = no expiration)
//! ```

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::modules::metrics::ALL_METRICS;
use crate::Error;
use crate::{Closer, MetricEntry, Metrics};
use async_trait::async_trait;
use chrono::Utc;
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Sender};
use serde::Deserialize;
use serde_yaml::Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::{debug, error, warn};

const DEFAULT_DATABASE: &str = "fiddler";
const DEFAULT_TABLE: &str = "metrics";
const DEFAULT_BATCH_SIZE: usize = 100;
const DEFAULT_FLUSH_INTERVAL_MS: u64 = 5000;
const DEFAULT_TTL_DAYS: u32 = 0;
// Buffer size is 10x batch size to handle bursts while batching provides efficiency
const CHANNEL_BUFFER_SIZE: usize = 1000;

/// ClickHouse dimension configuration.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct DimensionConfig {
    /// Dimension name (will be used as column name).
    pub name: String,
    /// Dimension value.
    pub value: String,
}

/// ClickHouse-specific configuration options.
#[derive(Debug, Deserialize, Clone)]
pub struct ClickHouseConfig {
    /// ClickHouse HTTP endpoint URL (required).
    pub url: String,
    /// Database name (default: "fiddler").
    #[serde(default = "default_database")]
    pub database: String,
    /// Table name (default: "metrics").
    #[serde(default = "default_table")]
    pub table: String,
    /// Username for authentication.
    pub username: Option<String>,
    /// Password for authentication.
    pub password: Option<String>,
    /// List of metric names to include. If empty or not set, all metrics are included.
    #[serde(default)]
    pub include: Vec<String>,
    /// List of metric names to exclude. Applied after include filter.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Additional dimensions to add to all metrics.
    #[serde(default)]
    pub dimensions: Vec<DimensionConfig>,
    /// Number of metrics to batch before sending (default: 100).
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Maximum time between flushes in milliseconds (default: 5000).
    #[serde(default = "default_flush_interval_ms")]
    pub flush_interval_ms: u64,
    /// Whether to auto-create the table if it doesn't exist (default: true).
    #[serde(default = "default_create_table")]
    pub create_table: bool,
    /// TTL (time-to-live) for metrics in days. Set to 0 to disable TTL (default: 0 = no expiration).
    #[serde(default = "default_ttl_days")]
    pub ttl_days: u32,
}

fn default_database() -> String {
    DEFAULT_DATABASE.to_string()
}

fn default_table() -> String {
    DEFAULT_TABLE.to_string()
}

fn default_batch_size() -> usize {
    DEFAULT_BATCH_SIZE
}

fn default_flush_interval_ms() -> u64 {
    DEFAULT_FLUSH_INTERVAL_MS
}

fn default_create_table() -> bool {
    true
}

fn default_ttl_days() -> u32 {
    DEFAULT_TTL_DAYS
}

/// Validate a ClickHouse identifier (database, table, column name).
/// Identifiers must contain only alphanumeric characters and underscores,
/// and must start with a letter or underscore.
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

/// Internal message type for the background publisher task.
enum PublisherMessage {
    Metric(MetricEntry),
    Shutdown,
}

/// ClickHouse HTTP client wrapper.
struct ClickHouseClient {
    client: reqwest::Client,
    url: String,
    database: String,
    table: String,
    username: Option<String>,
    password: Option<String>,
    dimensions: Vec<DimensionConfig>,
    include_set: HashSet<String>,
    exclude_set: HashSet<String>,
    table_created: AtomicBool,
    create_table_enabled: bool,
    ttl_days: u32,
}

impl ClickHouseClient {
    fn new(
        config: &ClickHouseConfig,
        include_set: HashSet<String>,
        exclude_set: HashSet<String>,
    ) -> Result<Self, Error> {
        // Validate identifiers to prevent SQL injection
        validate_identifier(&config.database)?;
        validate_identifier(&config.table)?;
        for dim in &config.dimensions {
            validate_identifier(&dim.name)?;
        }

        // Build HTTP client with proper configuration
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(2) // Keep connections alive for reuse
            .pool_idle_timeout(Duration::from_secs(90))
            .timeout(Duration::from_secs(30)) // Request timeout
            .connect_timeout(Duration::from_secs(10)) // Connection timeout
            .build()
            .map_err(|e| Error::ExecutionError(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self {
            client,
            url: config.url.clone(),
            database: config.database.clone(),
            table: config.table.clone(),
            username: config.username.clone(),
            password: config.password.clone(),
            dimensions: config.dimensions.clone(),
            include_set,
            exclude_set,
            table_created: AtomicBool::new(false),
            create_table_enabled: config.create_table,
            ttl_days: config.ttl_days,
        })
    }

    /// Execute a query against ClickHouse.
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
            .map_err(|e| Error::ExecutionError(format!("ClickHouse request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::ExecutionError(format!(
                "ClickHouse query failed with status {}: {}",
                status, body
            )));
        }

        Ok(())
    }

    /// Create the metrics table if it doesn't exist.
    async fn create_table_if_not_exists(&self) -> Result<(), Error> {
        // Build dimension columns
        let dimension_columns: String = self
            .dimensions
            .iter()
            .map(|d| format!("    {} String", d.name))
            .collect::<Vec<_>>()
            .join(",\n");

        let dimension_part = if dimension_columns.is_empty() {
            String::new()
        } else {
            format!(",\n{}", dimension_columns)
        };

        // Build TTL clause
        let ttl_clause = if self.ttl_days > 0 {
            format!(
                "\nTTL toDateTime(timestamp) + INTERVAL {} DAY",
                self.ttl_days
            )
        } else {
            String::new()
        };

        let create_query = format!(
            r#"CREATE TABLE IF NOT EXISTS {}.{} (
    timestamp DateTime64(3),
    metric_name String,
    metric_value Float64{}
) ENGINE = MergeTree()
ORDER BY (timestamp, metric_name){}"#,
            self.database, self.table, dimension_part, ttl_clause
        );

        self.execute_query(&create_query).await
    }

    /// Insert metrics into ClickHouse.
    async fn insert_metrics(&self, metrics: &[MetricEntry]) -> Result<(), Error> {
        if metrics.is_empty() {
            return Ok(());
        }

        // Retry table creation on first insert if it failed initially
        if self.create_table_enabled && !self.table_created.load(Ordering::Relaxed) {
            if self.create_table_if_not_exists().await.is_ok() {
                self.table_created.store(true, Ordering::Relaxed);
            }
        }

        // Build dimension column names
        let dimension_cols: String = self
            .dimensions
            .iter()
            .map(|d| format!(", {}", d.name))
            .collect();

        // Build the INSERT statement header
        let insert_header = format!(
            "INSERT INTO {}.{} (timestamp, metric_name, metric_value{}) VALUES ",
            self.database, self.table, dimension_cols
        );

        // Build dimension values once (same for all rows)
        let dimension_values: String = if self.dimensions.is_empty() {
            String::new()
        } else {
            self.dimensions
                .iter()
                .map(|d| format!(", '{}'", escape_string(&d.value)))
                .collect()
        };

        // Generate timestamp once per batch (all metrics in a batch represent the same snapshot)
        let timestamp = Utc::now().timestamp_millis();

        // Pre-allocate with estimated capacity (16 metric fields per entry)
        let estimated_rows = metrics.len() * 16;
        let mut rows = Vec::with_capacity(estimated_rows);

        for metric in metrics {
            self.add_metric_row(
                &mut rows,
                timestamp,
                "total_received",
                metric.total_received as f64,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "total_completed",
                metric.total_completed as f64,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "total_process_errors",
                metric.total_process_errors as f64,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "total_output_errors",
                metric.total_output_errors as f64,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "streams_started",
                metric.streams_started as f64,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "streams_completed",
                metric.streams_completed as f64,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "duplicates_rejected",
                metric.duplicates_rejected as f64,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "stale_entries_removed",
                metric.stale_entries_removed as f64,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "in_flight",
                metric.in_flight as f64,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "throughput_per_sec",
                metric.throughput_per_sec,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "input_bytes",
                metric.input_bytes as f64,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "output_bytes",
                metric.output_bytes as f64,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "bytes_per_sec",
                metric.bytes_per_sec,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "latency_avg_ms",
                metric.latency_avg_ms,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "latency_min_ms",
                metric.latency_min_ms,
                &dimension_values,
            );
            self.add_metric_row(
                &mut rows,
                timestamp,
                "latency_max_ms",
                metric.latency_max_ms,
                &dimension_values,
            );
        }

        if rows.is_empty() {
            return Ok(());
        }

        let query = format!("{}{}", insert_header, rows.join(", "));
        self.execute_query(&query).await
    }

    fn add_metric_row(
        &self,
        rows: &mut Vec<String>,
        timestamp: i64,
        metric_name: &str,
        value: f64,
        dimension_values: &str,
    ) {
        if self.should_include(metric_name) {
            rows.push(format!(
                "(fromUnixTimestamp64Milli({}), '{}', {}{})",
                timestamp, metric_name, value, dimension_values
            ));
        }
    }

    fn should_include(&self, metric_name: &str) -> bool {
        self.include_set.contains(metric_name) && !self.exclude_set.contains(metric_name)
    }
}

/// Escape a string for ClickHouse SQL.
fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// ClickHouse metrics backend.
///
/// Records metrics by sending them to ClickHouse asynchronously.
/// Uses a bounded channel with a background task for non-blocking operation.
/// Supports batching and periodic flushing for efficiency.
pub struct ClickHouseMetrics {
    sender: Sender<PublisherMessage>,
    include_set: HashSet<String>,
    exclude_set: HashSet<String>,
    shutdown_complete: Option<oneshot::Receiver<()>>,
}

impl ClickHouseMetrics {
    /// Creates a new ClickHouse metrics instance from configuration.
    pub async fn new(config: Value) -> Result<Self, Error> {
        let ch_config: ClickHouseConfig = serde_yaml::from_value(config)?;

        // Build include/exclude sets
        let include_set: HashSet<String> = if ch_config.include.is_empty() {
            ALL_METRICS.iter().map(|s| s.to_string()).collect()
        } else {
            ch_config.include.iter().cloned().collect()
        };

        let exclude_set: HashSet<String> = ch_config.exclude.iter().cloned().collect();

        let client = Arc::new(ClickHouseClient::new(
            &ch_config,
            include_set.clone(),
            exclude_set.clone(),
        )?);

        // Create table if configured
        if ch_config.create_table {
            match client.create_table_if_not_exists().await {
                Ok(()) => {
                    client.table_created.store(true, Ordering::Relaxed);
                }
                Err(e) => {
                    warn!(error = %e, "Failed to create ClickHouse table, will retry on first insert");
                }
            }
        }

        // Create channel for async publishing
        let (sender, receiver) = bounded::<PublisherMessage>(CHANNEL_BUFFER_SIZE);

        // Create shutdown completion channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let batch_size = ch_config.batch_size;
        let flush_interval = Duration::from_millis(ch_config.flush_interval_ms);

        // Spawn background task for publishing metrics
        tokio::spawn(async move {
            let mut batch: Vec<MetricEntry> = Vec::with_capacity(batch_size);
            let mut last_flush = std::time::Instant::now();

            loop {
                // Calculate remaining time until next flush
                let elapsed = last_flush.elapsed();
                let remaining = if elapsed >= flush_interval {
                    Duration::from_millis(1) // Use minimal timeout if overdue
                } else {
                    flush_interval - elapsed
                };

                let timeout = tokio::time::timeout(remaining, receiver.recv_async());

                match timeout.await {
                    Ok(Ok(PublisherMessage::Metric(metric))) => {
                        batch.push(metric);

                        // Flush if batch is full
                        if batch.len() >= batch_size {
                            if let Err(e) = client.insert_metrics(&batch).await {
                                error!(error = %e, "Failed to insert metrics to ClickHouse");
                            }
                            batch.clear();
                            last_flush = std::time::Instant::now();
                        }
                    }
                    Err(_) => {
                        // Timeout - flush on periodic interval
                        if !batch.is_empty() {
                            if let Err(e) = client.insert_metrics(&batch).await {
                                error!(error = %e, "Failed to insert metrics to ClickHouse");
                            }
                            batch.clear();
                        }
                        last_flush = std::time::Instant::now();
                    }
                    Ok(Ok(PublisherMessage::Shutdown)) => {
                        // Final flush before shutdown
                        if !batch.is_empty() {
                            if let Err(e) = client.insert_metrics(&batch).await {
                                error!(error = %e, "Failed to insert final metrics to ClickHouse");
                            }
                        }
                        debug!("ClickHouse metrics publisher task exiting");
                        let _ = shutdown_tx.send(());
                        break;
                    }
                    Ok(Err(_)) => {
                        // Channel closed unexpectedly
                        if !batch.is_empty() {
                            if let Err(e) = client.insert_metrics(&batch).await {
                                error!(error = %e, "Failed to insert final metrics to ClickHouse");
                            }
                        }
                        warn!("ClickHouse metrics publisher channel closed unexpectedly");
                        break;
                    }
                }
            }
        });

        debug!(
            url = %ch_config.url,
            database = %ch_config.database,
            table = %ch_config.table,
            "ClickHouse metrics backend initialized"
        );

        Ok(Self {
            sender,
            include_set,
            exclude_set,
            shutdown_complete: Some(shutdown_rx),
        })
    }

    /// Check if a metric should be included based on include/exclude filters.
    fn should_include(&self, metric_name: &str) -> bool {
        self.include_set.contains(metric_name) && !self.exclude_set.contains(metric_name)
    }
}

#[async_trait]
impl Metrics for ClickHouseMetrics {
    fn record(&mut self, metric: MetricEntry) {
        // Check if any metrics would be included
        let has_metrics = ALL_METRICS.iter().any(|name| self.should_include(name));

        if !has_metrics {
            return;
        }

        // Non-blocking send - warn if channel is full
        if let Err(e) = self.sender.try_send(PublisherMessage::Metric(metric)) {
            warn!(
                error = %e,
                buffer_size = CHANNEL_BUFFER_SIZE,
                "ClickHouse metrics channel full, dropping metric. \
                 Consider increasing batch_size, reducing flush_interval_ms, \
                 or checking ClickHouse connectivity"
            );
        }
    }
}

#[async_trait]
impl Closer for ClickHouseMetrics {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("ClickHouse metrics backend closing");

        // Send shutdown message to flush remaining metrics
        if let Err(e) = self.sender.send_async(PublisherMessage::Shutdown).await {
            warn!(error = %e, "Failed to send shutdown message");
            return Ok(());
        }

        // Wait for background task to complete with a timeout
        if let Some(rx) = self.shutdown_complete.take() {
            match tokio::time::timeout(Duration::from_secs(5), rx).await {
                Ok(Ok(())) => debug!("ClickHouse publisher shutdown complete"),
                Ok(Err(_)) => warn!("ClickHouse publisher shutdown channel closed unexpectedly"),
                Err(_) => warn!("ClickHouse publisher shutdown timed out after 5s"),
            }
        }

        Ok(())
    }
}

#[fiddler_registration_func]
fn create_clickhouse(conf: Value) -> Result<ExecutionType, Error> {
    Ok(ExecutionType::Metrics(Box::new(
        ClickHouseMetrics::new(conf).await?,
    )))
}

/// Registers the ClickHouse metrics plugin.
pub(crate) fn register_clickhouse() -> Result<(), Error> {
    let config = r#"type: object
required:
  - url
properties:
  url:
    type: string
    description: "ClickHouse HTTP endpoint URL"
  database:
    type: string
    default: "fiddler"
    description: "Database name"
  table:
    type: string
    default: "metrics"
    description: "Table name"
  username:
    type: string
    description: "Username for authentication"
  password:
    type: string
    description: "Password for authentication"
  include:
    type: array
    items:
      type: string
    description: "List of metric names to include"
  exclude:
    type: array
    items:
      type: string
    description: "List of metric names to exclude"
  dimensions:
    type: array
    items:
      type: object
      properties:
        name:
          type: string
        value:
          type: string
      required:
        - name
        - value
    description: "Additional dimensions for all metrics"
  batch_size:
    type: integer
    default: 100
    description: "Number of metrics to batch before sending"
  flush_interval_ms:
    type: integer
    default: 5000
    description: "Maximum time between flushes in milliseconds"
  create_table:
    type: boolean
    default: true
    description: "Auto-create table if not exists"
  ttl_days:
    type: integer
    default: 0
    description: "TTL for metrics in days (0 = no expiration)"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "clickhouse".into(),
        ItemType::Metrics,
        conf_spec,
        create_clickhouse,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        assert_eq!(default_database(), "fiddler");
        assert_eq!(default_table(), "metrics");
        assert_eq!(default_batch_size(), 100);
        assert_eq!(default_flush_interval_ms(), 5000);
        assert_eq!(default_ttl_days(), 0);
        assert!(default_create_table());
    }

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
url: "http://localhost:8123"
database: "test_db"
table: "test_metrics"
username: "user"
password: "pass"
include:
  - total_received
  - throughput_per_sec
exclude:
  - stale_entries_removed
dimensions:
  - name: environment
    value: production
batch_size: 50
flush_interval_ms: 1000
create_table: false
ttl_days: 7
"#;
        let config: ClickHouseConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url, "http://localhost:8123");
        assert_eq!(config.database, "test_db");
        assert_eq!(config.table, "test_metrics");
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
        assert_eq!(config.include.len(), 2);
        assert_eq!(config.exclude.len(), 1);
        assert_eq!(config.dimensions.len(), 1);
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.flush_interval_ms, 1000);
        assert_eq!(config.ttl_days, 7);
        assert!(!config.create_table);
    }

    #[test]
    fn test_config_defaults() {
        let yaml = r#"url: "http://localhost:8123""#;
        let config: ClickHouseConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url, "http://localhost:8123");
        assert_eq!(config.database, "fiddler");
        assert_eq!(config.table, "metrics");
        assert!(config.username.is_none());
        assert!(config.password.is_none());
        assert!(config.include.is_empty());
        assert!(config.exclude.is_empty());
        assert!(config.dimensions.is_empty());
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.flush_interval_ms, 5000);
        assert_eq!(config.ttl_days, 0);
        assert!(config.create_table);
    }

    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string("hello"), "hello");
        assert_eq!(escape_string("it's"), "it\\'s");
        assert_eq!(escape_string("back\\slash"), "back\\\\slash");
        assert_eq!(escape_string("it's a \\test"), "it\\'s a \\\\test");
        assert_eq!(escape_string(""), "");
        assert_eq!(escape_string("'"), "\\'");
        assert_eq!(escape_string("\\"), "\\\\");
    }

    #[test]
    fn test_validate_identifier_valid() {
        assert!(validate_identifier("fiddler").is_ok());
        assert!(validate_identifier("my_database").is_ok());
        assert!(validate_identifier("_private").is_ok());
        assert!(validate_identifier("Table123").is_ok());
        assert!(validate_identifier("a").is_ok());
    }

    #[test]
    fn test_validate_identifier_invalid() {
        assert!(validate_identifier("").is_err());
        assert!(validate_identifier("123invalid").is_err());
        assert!(validate_identifier("invalid-name").is_err());
        assert!(validate_identifier("invalid.name").is_err());
        assert!(validate_identifier("invalid name").is_err());
        assert!(validate_identifier("invalid;name").is_err());
    }

    #[test]
    fn test_register_clickhouse() {
        let result = register_clickhouse();
        // May return DuplicateRegisteredName if already registered by another test
        assert!(result.is_ok() || matches!(result, Err(crate::Error::DuplicateRegisteredName(_))));
    }

    #[test]
    fn test_should_include_all_metrics() {
        let include_set: HashSet<String> = ALL_METRICS.iter().map(|s| s.to_string()).collect();
        let exclude_set: HashSet<String> = HashSet::new();

        let config = ClickHouseConfig {
            url: "http://localhost:8123".to_string(),
            database: default_database(),
            table: default_table(),
            username: None,
            password: None,
            include: vec![],
            exclude: vec![],
            dimensions: vec![],
            batch_size: default_batch_size(),
            flush_interval_ms: default_flush_interval_ms(),
            create_table: false,
            ttl_days: default_ttl_days(),
        };

        let client = ClickHouseClient::new(&config, include_set, exclude_set).unwrap();

        for metric in ALL_METRICS {
            assert!(client.should_include(metric));
        }
    }

    #[test]
    fn test_should_include_with_filter() {
        let include_set: HashSet<String> = vec![
            "total_received".to_string(),
            "throughput_per_sec".to_string(),
        ]
        .into_iter()
        .collect();
        let exclude_set: HashSet<String> = HashSet::new();

        let config = ClickHouseConfig {
            url: "http://localhost:8123".to_string(),
            database: default_database(),
            table: default_table(),
            username: None,
            password: None,
            include: vec![],
            exclude: vec![],
            dimensions: vec![],
            batch_size: default_batch_size(),
            flush_interval_ms: default_flush_interval_ms(),
            create_table: false,
            ttl_days: default_ttl_days(),
        };

        let client = ClickHouseClient::new(&config, include_set, exclude_set).unwrap();

        assert!(client.should_include("total_received"));
        assert!(client.should_include("throughput_per_sec"));
        assert!(!client.should_include("total_completed"));
        assert!(!client.should_include("in_flight"));
    }

    #[test]
    fn test_should_include_with_exclude() {
        let include_set: HashSet<String> = ALL_METRICS.iter().map(|s| s.to_string()).collect();
        let exclude_set: HashSet<String> = vec!["stale_entries_removed".to_string()]
            .into_iter()
            .collect();

        let config = ClickHouseConfig {
            url: "http://localhost:8123".to_string(),
            database: default_database(),
            table: default_table(),
            username: None,
            password: None,
            include: vec![],
            exclude: vec![],
            dimensions: vec![],
            batch_size: default_batch_size(),
            flush_interval_ms: default_flush_interval_ms(),
            create_table: false,
            ttl_days: default_ttl_days(),
        };

        let client = ClickHouseClient::new(&config, include_set, exclude_set).unwrap();

        assert!(client.should_include("total_received"));
        assert!(!client.should_include("stale_entries_removed"));
    }

    #[test]
    fn test_invalid_database_name() {
        let include_set: HashSet<String> = ALL_METRICS.iter().map(|s| s.to_string()).collect();
        let exclude_set: HashSet<String> = HashSet::new();

        let config = ClickHouseConfig {
            url: "http://localhost:8123".to_string(),
            database: "invalid-db".to_string(), // Contains hyphen
            table: default_table(),
            username: None,
            password: None,
            include: vec![],
            exclude: vec![],
            dimensions: vec![],
            batch_size: default_batch_size(),
            flush_interval_ms: default_flush_interval_ms(),
            create_table: false,
            ttl_days: default_ttl_days(),
        };

        let result = ClickHouseClient::new(&config, include_set, exclude_set);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_dimension_name() {
        let include_set: HashSet<String> = ALL_METRICS.iter().map(|s| s.to_string()).collect();
        let exclude_set: HashSet<String> = HashSet::new();

        let config = ClickHouseConfig {
            url: "http://localhost:8123".to_string(),
            database: default_database(),
            table: default_table(),
            username: None,
            password: None,
            include: vec![],
            exclude: vec![],
            dimensions: vec![DimensionConfig {
                name: "invalid.name".to_string(),
                value: "value".to_string(),
            }],
            batch_size: default_batch_size(),
            flush_interval_ms: default_flush_interval_ms(),
            create_table: false,
            ttl_days: default_ttl_days(),
        };

        let result = ClickHouseClient::new(&config, include_set, exclude_set);
        assert!(result.is_err());
    }
}
