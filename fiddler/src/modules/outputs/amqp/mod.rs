//! AMQP output module for publishing to exchanges.
//!
//! # Configuration
//!
//! ```yaml
//! output:
//!   amqp:
//!     url: "amqp://guest:guest@localhost:5672/%2f"  # Required
//!     exchange: "events"                             # Required
//!     routing_key: "fiddler.output"                  # Optional (default: "")
//!     mandatory: false                               # Optional
//!     persistent: true                               # Optional (default: true)
//!     batch:
//!       size: 500
//!       duration: "10s"
//! ```

use crate::config::{register_plugin, ConfigSpec, ExecutionType, ItemType};
use crate::{BatchingPolicy, Closer, Error, MessageBatch, OutputBatch};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use lapin::{
    options::BasicPublishOptions,
    BasicProperties, Channel, Connection, ConnectionProperties,
};
use serde::Deserialize;
use serde_yaml::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::debug;

#[derive(Deserialize, Clone)]
pub struct AmqpOutputConfig {
    pub url: String,
    pub exchange: String,
    #[serde(default)]
    pub routing_key: String,
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default = "default_persistent")]
    pub persistent: bool,
    #[serde(default)]
    pub batch: Option<BatchingPolicy>,
}

fn default_persistent() -> bool {
    true
}

struct AmqpClient {
    url: String,
    exchange: String,
    routing_key: String,
    mandatory: bool,
    persistent: bool,
    channel: Option<Channel>,
}

impl AmqpClient {
    fn new(config: &AmqpOutputConfig) -> Self {
        Self {
            url: config.url.clone(),
            exchange: config.exchange.clone(),
            routing_key: config.routing_key.clone(),
            mandatory: config.mandatory,
            persistent: config.persistent,
            channel: None,
        }
    }

    async fn ensure_connected(&mut self) -> Result<&Channel, Error> {
        if self.channel.as_ref().map_or(true, |c| !c.status().connected()) {
            let conn = Connection::connect(&self.url, ConnectionProperties::default())
                .await
                .map_err(|e| Error::OutputError(format!("AMQP connection failed: {}", e)))?;

            let channel = conn
                .create_channel()
                .await
                .map_err(|e| Error::OutputError(format!("AMQP channel failed: {}", e)))?;

            self.channel = Some(channel);
        }
        Ok(self.channel.as_ref().expect("channel should be set"))
    }

    async fn publish_batch(&mut self, messages: &MessageBatch) -> Result<(), Error> {
        // Extract values before mutable borrow
        let exchange = self.exchange.clone();
        let routing_key = self.routing_key.clone();
        let mandatory = self.mandatory;
        let persistent = self.persistent;

        let channel = self.ensure_connected().await?;

        let properties = if persistent {
            BasicProperties::default().with_delivery_mode(2)
        } else {
            BasicProperties::default()
        };

        let options = BasicPublishOptions {
            mandatory,
            ..Default::default()
        };

        for msg in messages {
            channel
                .basic_publish(
                    &exchange,
                    &routing_key,
                    options,
                    &msg.bytes,
                    properties.clone(),
                )
                .await
                .map_err(|e| Error::OutputError(format!("AMQP publish failed: {}", e)))?;
        }

        debug!(count = messages.len(), "Published batch to AMQP");
        Ok(())
    }
}

pub struct AmqpOutput {
    client: Arc<Mutex<AmqpClient>>,
    batch_size: usize,
    interval: Duration,
}

impl AmqpOutput {
    pub fn new(config: AmqpOutputConfig) -> Result<Self, Error> {
        let batch_size = config.batch.as_ref().map_or(500, |b| b.effective_size());
        let interval = config
            .batch
            .as_ref()
            .map_or(Duration::from_secs(10), |b| b.effective_duration());

        let client = AmqpClient::new(&config);

        debug!(exchange = %config.exchange, "AMQP output initialized");

        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            batch_size,
            interval,
        })
    }
}

#[async_trait]
impl OutputBatch for AmqpOutput {
    async fn write_batch(&mut self, messages: MessageBatch) -> Result<(), Error> {
        if messages.is_empty() {
            return Ok(());
        }

        let mut client = self.client.lock().await;
        client.publish_batch(&messages).await
    }

    async fn batch_size(&self) -> usize {
        self.batch_size
    }

    async fn interval(&self) -> Duration {
        self.interval
    }
}

#[async_trait]
impl Closer for AmqpOutput {
    async fn close(&mut self) -> Result<(), Error> {
        if let Some(channel) = self.client.lock().await.channel.take() {
            let _ = channel.close(200, "Normal shutdown").await;
        }
        debug!("AMQP output closed");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_amqp_output(conf: Value) -> Result<ExecutionType, Error> {
    let config: AmqpOutputConfig = serde_yaml::from_value(conf)?;

    if config.url.is_empty() {
        return Err(Error::ConfigFailedValidation("url is required".into()));
    }
    if config.exchange.is_empty() {
        return Err(Error::ConfigFailedValidation("exchange is required".into()));
    }

    Ok(ExecutionType::OutputBatch(Box::new(AmqpOutput::new(config)?)))
}

pub(crate) fn register_amqp() -> Result<(), Error> {
    let config = r#"type: object
required:
  - url
  - exchange
properties:
  url:
    type: string
    description: "AMQP connection URL"
  exchange:
    type: string
    description: "Exchange to publish to"
  routing_key:
    type: string
    default: ""
    description: "Routing key"
  mandatory:
    type: boolean
    default: false
    description: "Mandatory flag"
  persistent:
    type: boolean
    default: true
    description: "Persistent delivery mode"
  batch:
    type: object
    properties:
      size:
        type: integer
      duration:
        type: string
    description: "Batching configuration"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;
    register_plugin("amqp".into(), ItemType::OutputBatch, conf_spec, create_amqp_output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
url: "amqp://localhost"
exchange: "events"
routing_key: "test.key"
mandatory: true
persistent: false
"#;
        let config: AmqpOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.exchange, "events");
        assert_eq!(config.routing_key, "test.key");
        assert!(config.mandatory);
        assert!(!config.persistent);
    }

    #[test]
    fn test_config_defaults() {
        let yaml = r#"
url: "amqp://localhost"
exchange: "test"
"#;
        let config: AmqpOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.routing_key.is_empty());
        assert!(!config.mandatory);
        assert!(config.persistent);
    }

    #[test]
    fn test_register() {
        let result = register_amqp();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
