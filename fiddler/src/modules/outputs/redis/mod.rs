//! Redis output module for publishing data.
//!
//! This module provides output implementations for sending data to Redis
//! using either list operations (LPUSH/RPUSH) or pub/sub (PUBLISH).
//!
//! # Configuration
//!
//! ```yaml
//! output:
//!   redis:
//!     url: "redis://localhost:6379/0"   # Required: Redis connection URL
//!     mode: "list"                       # Optional: "list" or "pubsub" (default: "list")
//!     key: "events"                      # Required for list mode: key name
//!     channel: "events"                  # Required for pubsub mode: channel name
//!     list_command: "rpush"              # Optional: "lpush" or "rpush" (default: "rpush")
//!     batch:                             # Optional: Batching policy (list mode only)
//!       size: 500
//!       duration: "5s"
//! ```

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::{BatchingPolicy, Closer, Error, Message, MessageBatch, Output, OutputBatch};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client};
use serde::Deserialize;
use serde_yaml::Value;
use std::time::Duration;
use tracing::{debug, warn};

const DEFAULT_MODE: &str = "list";
const DEFAULT_LIST_COMMAND: &str = "rpush";

/// Redis output configuration.
#[derive(Deserialize, Clone)]
pub struct RedisOutputConfig {
    /// Redis connection URL (required).
    /// Format: redis://[username:password@]host[:port][/db]
    pub url: String,
    /// Operation mode: "list" or "pubsub" (default: "list").
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Key name for list mode.
    pub key: Option<String>,
    /// Channel name for pubsub mode.
    pub channel: Option<String>,
    /// List command: "lpush" or "rpush" (default: "rpush").
    #[serde(default = "default_list_command")]
    pub list_command: String,
    /// Batching policy (list mode only).
    #[serde(default)]
    pub batch: Option<BatchingPolicy>,
}

fn default_mode() -> String {
    DEFAULT_MODE.to_string()
}

fn default_list_command() -> String {
    DEFAULT_LIST_COMMAND.to_string()
}

/// Redis list output using batch operations with pipelining.
pub struct RedisListOutput {
    conn: ConnectionManager,
    key: String,
    use_lpush: bool,
    batch_size: usize,
    interval: Duration,
}

impl RedisListOutput {
    /// Creates a new Redis list output.
    pub async fn new(config: RedisOutputConfig) -> Result<Self, Error> {
        let key = config
            .key
            .ok_or_else(|| Error::ConfigFailedValidation("key required for list mode".into()))?;

        let use_lpush = config.list_command.to_lowercase() == "lpush";

        let client = Client::open(config.url.as_str())
            .map_err(|e| Error::ConfigFailedValidation(format!("Invalid Redis URL: {}", e)))?;

        let conn = ConnectionManager::new(client)
            .await
            .map_err(|e| Error::ExecutionError(format!("Failed to connect to Redis: {}", e)))?;

        let batch_size = config.batch.as_ref().map_or(500, |b| b.effective_size());
        let interval = config
            .batch
            .as_ref()
            .map_or(Duration::from_secs(10), |b| b.effective_duration());

        debug!(key = %key, use_lpush = use_lpush, "Redis list output initialized");

        Ok(Self {
            conn,
            key,
            use_lpush,
            batch_size,
            interval,
        })
    }
}

#[async_trait]
impl OutputBatch for RedisListOutput {
    async fn write_batch(&mut self, messages: MessageBatch) -> Result<(), Error> {
        if messages.is_empty() {
            return Ok(());
        }

        // Use pipelining for efficient batch inserts
        let mut pipe = redis::pipe();

        for msg in &messages {
            if self.use_lpush {
                pipe.lpush(&self.key, msg.bytes.as_slice());
            } else {
                pipe.rpush(&self.key, msg.bytes.as_slice());
            }
        }

        pipe.query_async::<()>(&mut self.conn)
            .await
            .map_err(|e| Error::OutputError(format!("Redis push failed: {}", e)))?;

        debug!(count = messages.len(), key = %self.key, "Pushed batch to Redis");
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
impl Closer for RedisListOutput {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("Redis list output closing");
        Ok(())
    }
}

/// Redis pub/sub output for publishing messages.
pub struct RedisPubSubOutput {
    conn: ConnectionManager,
    channel: String,
}

impl RedisPubSubOutput {
    /// Creates a new Redis pub/sub output.
    pub async fn new(config: RedisOutputConfig) -> Result<Self, Error> {
        let channel = config.channel.ok_or_else(|| {
            Error::ConfigFailedValidation("channel required for pubsub mode".into())
        })?;

        let client = Client::open(config.url.as_str())
            .map_err(|e| Error::ConfigFailedValidation(format!("Invalid Redis URL: {}", e)))?;

        let conn = ConnectionManager::new(client)
            .await
            .map_err(|e| Error::ExecutionError(format!("Failed to connect to Redis: {}", e)))?;

        debug!(channel = %channel, "Redis pub/sub output initialized");

        Ok(Self { conn, channel })
    }
}

#[async_trait]
impl Output for RedisPubSubOutput {
    async fn write(&mut self, message: Message) -> Result<(), Error> {
        self.conn
            .publish::<_, _, ()>(&self.channel, message.bytes.as_slice())
            .await
            .map_err(|e| Error::OutputError(format!("Redis publish failed: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl Closer for RedisPubSubOutput {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("Redis pub/sub output closing");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_redis_output(conf: Value) -> Result<ExecutionType, Error> {
    let config: RedisOutputConfig = serde_yaml::from_value(conf)?;

    // Validate URL
    if config.url.is_empty() {
        return Err(Error::ConfigFailedValidation("url is required".into()));
    }

    // Validate mode-specific requirements
    match config.mode.as_str() {
        "list" => {
            if config.key.is_none() {
                return Err(Error::ConfigFailedValidation(
                    "key is required for list mode".into(),
                ));
            }
            if !["lpush", "rpush"].contains(&config.list_command.to_lowercase().as_str()) {
                return Err(Error::ConfigFailedValidation(
                    "list_command must be 'lpush' or 'rpush'".into(),
                ));
            }
            Ok(ExecutionType::OutputBatch(Box::new(
                RedisListOutput::new(config).await?,
            )))
        }
        "pubsub" => {
            if config.channel.is_none() {
                return Err(Error::ConfigFailedValidation(
                    "channel is required for pubsub mode".into(),
                ));
            }
            if config.batch.is_some() {
                warn!("batch configuration is ignored in pubsub mode");
            }
            Ok(ExecutionType::Output(Box::new(
                RedisPubSubOutput::new(config).await?,
            )))
        }
        _ => Err(Error::ConfigFailedValidation(
            "mode must be 'list' or 'pubsub'".into(),
        )),
    }
}

/// Registers the Redis output plugin.
pub(crate) fn register_redis() -> Result<(), Error> {
    let config = r#"type: object
required:
  - url
properties:
  url:
    type: string
    description: "Redis connection URL (redis://host:port/db)"
  mode:
    type: string
    enum: ["list", "pubsub"]
    default: "list"
    description: "Operation mode"
  key:
    type: string
    description: "List key name (required for list mode)"
  channel:
    type: string
    description: "Pub/Sub channel name (required for pubsub mode)"
  list_command:
    type: string
    enum: ["lpush", "rpush"]
    default: "rpush"
    description: "List operation (for list mode)"
  batch:
    type: object
    properties:
      size:
        type: integer
        description: "Batch size (default: 500)"
      duration:
        type: string
        description: "Flush interval (default: 10s)"
    description: "Batching policy (list mode only)"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    // Register for both Output (pubsub mode) and OutputBatch (list mode)
    // The create_redis_output function returns the appropriate type based on mode
    register_plugin(
        "redis".into(),
        ItemType::Output,
        conf_spec.clone(),
        create_redis_output,
    )?;
    register_plugin(
        "redis".into(),
        ItemType::OutputBatch,
        conf_spec,
        create_redis_output,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization_list() {
        let yaml = r#"
url: "redis://localhost:6379/0"
mode: "list"
key: "events"
list_command: "rpush"
batch:
  size: 1000
"#;
        let config: RedisOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url, "redis://localhost:6379/0");
        assert_eq!(config.mode, "list");
        assert_eq!(config.key, Some("events".to_string()));
        assert_eq!(config.list_command, "rpush");
        assert!(config.batch.is_some());
    }

    #[test]
    fn test_config_deserialization_pubsub() {
        let yaml = r#"
url: "redis://localhost:6379"
mode: "pubsub"
channel: "events.stream"
"#;
        let config: RedisOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url, "redis://localhost:6379");
        assert_eq!(config.mode, "pubsub");
        assert_eq!(config.channel, Some("events.stream".to_string()));
    }

    #[test]
    fn test_config_defaults() {
        let yaml = r#"
url: "redis://localhost:6379"
key: "test"
"#;
        let config: RedisOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.mode, "list");
        assert_eq!(config.list_command, "rpush");
        assert!(config.batch.is_none());
    }

    #[test]
    fn test_register_redis() {
        let result = register_redis();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
