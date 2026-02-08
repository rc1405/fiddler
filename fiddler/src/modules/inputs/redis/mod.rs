//! Redis input module for consuming data.
//!
//! This module provides input implementations for reading data from Redis
//! using either blocking list operations (BLPOP/BRPOP) or pub/sub subscriptions.
//!
//! # Configuration
//!
//! ```yaml
//! input:
//!   redis:
//!     url: "redis://localhost:6379/0"   # Required: Redis connection URL
//!     mode: "list"                       # Optional: "list" or "pubsub" (default: "list")
//!     keys:                              # Required for list mode: key names
//!       - "input_queue"
//!     channels:                          # Required for pubsub mode: channels/patterns
//!       - "events.*"
//!     list_command: "brpop"              # Optional: "blpop" or "brpop" (default: "brpop")
//!     timeout: 1                         # Optional: blocking timeout in seconds (default: 1)
//!     use_patterns: false                # Optional: use pattern matching for pubsub (default: false)
//! ```

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::{CallbackChan, Closer, Error, Input, Message};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, Sender};
use futures::StreamExt;
use redis::Client;
use serde::Deserialize;
use serde_yaml::Value;
use tokio::sync::oneshot;
use tracing::{debug, error, warn};

const DEFAULT_MODE: &str = "list";
const DEFAULT_LIST_COMMAND: &str = "brpop";
const DEFAULT_TIMEOUT: u64 = 1;
const CHANNEL_BUFFER_SIZE: usize = 100;

/// Redis input configuration.
#[derive(Deserialize, Clone)]
pub struct RedisInputConfig {
    /// Redis connection URL (required).
    pub url: String,
    /// Operation mode: "list" or "pubsub" (default: "list").
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Key names for list mode.
    pub keys: Option<Vec<String>>,
    /// Channel names/patterns for pubsub mode.
    pub channels: Option<Vec<String>>,
    /// List command: "blpop" or "brpop" (default: "brpop").
    #[serde(default = "default_list_command")]
    pub list_command: String,
    /// Blocking timeout in seconds (default: 1).
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// Use pattern matching for pubsub (default: false).
    #[serde(default)]
    pub use_patterns: bool,
}

fn default_mode() -> String {
    DEFAULT_MODE.to_string()
}

fn default_list_command() -> String {
    DEFAULT_LIST_COMMAND.to_string()
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT
}

/// Background task for reading from Redis lists.
async fn list_reader_task(
    config: RedisInputConfig,
    sender: Sender<Result<Message, Error>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let keys = config.keys.unwrap_or_default();
    let use_blpop = config.list_command.to_lowercase() == "blpop";
    let timeout_secs = config.timeout;

    let client = match Client::open(config.url.as_str()) {
        Ok(c) => c,
        Err(e) => {
            let _ = sender
                .send_async(Err(Error::ExecutionError(format!(
                    "Failed to open Redis client: {}",
                    e
                ))))
                .await;
            return;
        }
    };

    let mut conn = match client.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(e) => {
            let _ = sender
                .send_async(Err(Error::ExecutionError(format!(
                    "Failed to connect to Redis: {}",
                    e
                ))))
                .await;
            return;
        }
    };

    debug!(keys = ?keys, use_blpop = use_blpop, "Redis list reader started");

    loop {
        // Check for shutdown
        match shutdown_rx.try_recv() {
            Ok(_) | Err(oneshot::error::TryRecvError::Closed) => {
                debug!("Redis list reader received shutdown signal");
                let _ = sender.send_async(Err(Error::EndOfInput)).await;
                return;
            }
            Err(oneshot::error::TryRecvError::Empty) => {}
        }

        // Build the command
        let result: Result<Option<(String, Vec<u8>)>, redis::RedisError> = if use_blpop {
            redis::cmd("BLPOP")
                .arg(&keys)
                .arg(timeout_secs)
                .query_async(&mut conn)
                .await
        } else {
            redis::cmd("BRPOP")
                .arg(&keys)
                .arg(timeout_secs)
                .query_async(&mut conn)
                .await
        };

        match result {
            Ok(Some((_key, data))) => {
                let msg = Message {
                    bytes: data,
                    ..Default::default()
                };
                if sender.send_async(Ok(msg)).await.is_err() {
                    debug!("Redis list reader channel closed");
                    return;
                }
            }
            Ok(None) => {
                // Timeout, no data available - continue loop
            }
            Err(e) => {
                error!(error = %e, "Redis BRPOP/BLPOP failed");
                // Try to reconnect
                match client.get_multiplexed_async_connection().await {
                    Ok(new_conn) => {
                        conn = new_conn;
                        warn!("Reconnected to Redis");
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to reconnect to Redis");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        }
    }
}

/// Background task for reading from Redis pub/sub.
async fn pubsub_reader_task(
    config: RedisInputConfig,
    sender: Sender<Result<Message, Error>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let channels = config.channels.unwrap_or_default();
    let use_patterns = config.use_patterns;

    let client = match Client::open(config.url.as_str()) {
        Ok(c) => c,
        Err(e) => {
            let _ = sender
                .send_async(Err(Error::ExecutionError(format!(
                    "Failed to open Redis client: {}",
                    e
                ))))
                .await;
            return;
        }
    };

    let mut pubsub = match client.get_async_pubsub().await {
        Ok(ps) => ps,
        Err(e) => {
            let _ = sender
                .send_async(Err(Error::ExecutionError(format!(
                    "Failed to create pub/sub connection: {}",
                    e
                ))))
                .await;
            return;
        }
    };

    // Subscribe to channels
    for channel in &channels {
        let result = if use_patterns {
            pubsub.psubscribe(channel).await
        } else {
            pubsub.subscribe(channel).await
        };

        if let Err(e) = result {
            let _ = sender
                .send_async(Err(Error::ExecutionError(format!(
                    "Failed to subscribe to {}: {}",
                    channel, e
                ))))
                .await;
            return;
        }
    }

    debug!(channels = ?channels, use_patterns = use_patterns, "Redis pub/sub reader started");

    loop {
        // Check for shutdown
        match shutdown_rx.try_recv() {
            Ok(_) | Err(oneshot::error::TryRecvError::Closed) => {
                debug!("Redis pub/sub reader received shutdown signal");
                let _ = sender.send_async(Err(Error::EndOfInput)).await;
                return;
            }
            Err(oneshot::error::TryRecvError::Empty) => {}
        }

        // Use timeout to allow checking shutdown
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            pubsub.on_message().next(),
        )
        .await;

        match result {
            Ok(Some(msg)) => {
                let payload: Vec<u8> = match msg.get_payload_bytes() {
                    bytes => bytes.to_vec(),
                };
                let message = Message {
                    bytes: payload,
                    ..Default::default()
                };
                if sender.send_async(Ok(message)).await.is_err() {
                    debug!("Redis pub/sub reader channel closed");
                    return;
                }
            }
            Ok(None) => {
                // Stream ended
                debug!("Redis pub/sub stream ended");
                let _ = sender.send_async(Err(Error::EndOfInput)).await;
                return;
            }
            Err(_) => {
                // Timeout, continue loop to check shutdown
            }
        }
    }
}

/// Redis input reader.
pub struct RedisInput {
    receiver: Receiver<Result<Message, Error>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl RedisInput {
    /// Creates a new Redis input.
    pub fn new(config: RedisInputConfig) -> Result<Self, Error> {
        let (sender, receiver) = bounded(CHANNEL_BUFFER_SIZE);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let mode = config.mode.clone();

        match mode.as_str() {
            "list" => {
                tokio::spawn(list_reader_task(config, sender, shutdown_rx));
            }
            "pubsub" => {
                tokio::spawn(pubsub_reader_task(config, sender, shutdown_rx));
            }
            _ => {
                return Err(Error::ConfigFailedValidation(
                    "mode must be 'list' or 'pubsub'".into(),
                ));
            }
        }

        Ok(Self {
            receiver,
            shutdown_tx: Some(shutdown_tx),
        })
    }
}

#[async_trait]
impl Input for RedisInput {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        match self.receiver.recv_async().await {
            Ok(Ok(msg)) => Ok((msg, None)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::EndOfInput),
        }
    }
}

#[async_trait]
impl Closer for RedisInput {
    async fn close(&mut self) -> Result<(), Error> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        debug!("Redis input closing");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_redis_input(conf: Value) -> Result<ExecutionType, Error> {
    let config: RedisInputConfig = serde_yaml::from_value(conf)?;

    // Validate URL
    if config.url.is_empty() {
        return Err(Error::ConfigFailedValidation("url is required".into()));
    }

    // Validate mode-specific requirements
    match config.mode.as_str() {
        "list" => {
            if config.keys.is_none() || config.keys.as_ref().is_some_and(|k| k.is_empty()) {
                return Err(Error::ConfigFailedValidation(
                    "keys is required for list mode".into(),
                ));
            }
            if !["blpop", "brpop"].contains(&config.list_command.to_lowercase().as_str()) {
                return Err(Error::ConfigFailedValidation(
                    "list_command must be 'blpop' or 'brpop'".into(),
                ));
            }
        }
        "pubsub" => {
            if config.channels.is_none() || config.channels.as_ref().is_some_and(|c| c.is_empty()) {
                return Err(Error::ConfigFailedValidation(
                    "channels is required for pubsub mode".into(),
                ));
            }
        }
        _ => {
            return Err(Error::ConfigFailedValidation(
                "mode must be 'list' or 'pubsub'".into(),
            ));
        }
    }

    Ok(ExecutionType::Input(Box::new(RedisInput::new(config)?)))
}

/// Registers the Redis input plugin.
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
  keys:
    type: array
    items:
      type: string
    description: "List key names to pop from (required for list mode)"
  channels:
    type: array
    items:
      type: string
    description: "Pub/Sub channels to subscribe (required for pubsub mode)"
  list_command:
    type: string
    enum: ["blpop", "brpop"]
    default: "brpop"
    description: "Blocking list operation (for list mode)"
  timeout:
    type: integer
    default: 1
    description: "Blocking timeout in seconds"
  use_patterns:
    type: boolean
    default: false
    description: "Use pattern matching for channels (pubsub mode)"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "redis".into(),
        ItemType::Input,
        conf_spec,
        create_redis_input,
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
keys:
  - "input_queue"
  - "fallback_queue"
list_command: "brpop"
timeout: 5
"#;
        let config: RedisInputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url, "redis://localhost:6379/0");
        assert_eq!(config.mode, "list");
        assert_eq!(
            config.keys,
            Some(vec![
                "input_queue".to_string(),
                "fallback_queue".to_string()
            ])
        );
        assert_eq!(config.list_command, "brpop");
        assert_eq!(config.timeout, 5);
    }

    #[test]
    fn test_config_deserialization_pubsub() {
        let yaml = r#"
url: "redis://localhost:6379"
mode: "pubsub"
channels:
  - "events.*"
  - "logs.*"
use_patterns: true
"#;
        let config: RedisInputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url, "redis://localhost:6379");
        assert_eq!(config.mode, "pubsub");
        assert_eq!(
            config.channels,
            Some(vec!["events.*".to_string(), "logs.*".to_string()])
        );
        assert!(config.use_patterns);
    }

    #[test]
    fn test_config_defaults() {
        let yaml = r#"
url: "redis://localhost:6379"
keys:
  - "test"
"#;
        let config: RedisInputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.mode, "list");
        assert_eq!(config.list_command, "brpop");
        assert_eq!(config.timeout, 1);
        assert!(!config.use_patterns);
    }

    #[test]
    fn test_register_redis() {
        let result = register_redis();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
