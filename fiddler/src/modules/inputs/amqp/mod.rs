//! AMQP input module for consuming from queues.
//!
//! # Configuration
//!
//! ```yaml
//! input:
//!   amqp:
//!     url: "amqp://guest:guest@localhost:5672/%2f"  # Required
//!     queue: "events"                                # Required
//!     consumer_tag: "fiddler"                        # Optional
//!     prefetch_count: 100                            # Optional (default: 100)
//!     auto_ack: false                                # Optional (default: false)
//! ```

use crate::config::{register_plugin, ConfigSpec, ExecutionType, ItemType};
use crate::{CallbackChan, Closer, Error, Input, Message};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, Sender};
use futures::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicQosOptions},
    types::FieldTable,
    Connection, ConnectionProperties,
};
use serde::Deserialize;
use serde_yaml::Value;
use tokio::sync::oneshot;
use tracing::{debug, error, warn};

const DEFAULT_PREFETCH: u16 = 100;
const CHANNEL_BUFFER: usize = 100;

#[derive(Deserialize, Clone)]
pub struct AmqpInputConfig {
    pub url: String,
    pub queue: String,
    #[serde(default = "default_consumer_tag")]
    pub consumer_tag: String,
    #[serde(default = "default_prefetch")]
    pub prefetch_count: u16,
    #[serde(default)]
    pub auto_ack: bool,
}

fn default_consumer_tag() -> String {
    "fiddler".to_string()
}

fn default_prefetch() -> u16 {
    DEFAULT_PREFETCH
}

struct DeliveryInfo {
    delivery_tag: u64,
    channel: lapin::Channel,
}

async fn amqp_reader_task(
    config: AmqpInputConfig,
    sender: Sender<Result<(Message, Option<DeliveryInfo>), Error>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    loop {
        if shutdown_rx.try_recv().is_ok() {
            let _ = sender.send_async(Err(Error::EndOfInput)).await;
            return;
        }

        let conn = match Connection::connect(&config.url, ConnectionProperties::default()).await {
            Ok(c) => c,
            Err(e) => {
                error!(error = %e, "AMQP connection failed");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let channel = match conn.create_channel().await {
            Ok(c) => c,
            Err(e) => {
                error!(error = %e, "AMQP channel creation failed");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        if let Err(e) = channel
            .basic_qos(config.prefetch_count, BasicQosOptions::default())
            .await
        {
            warn!(error = %e, "Failed to set QoS");
        }

        let mut consumer = match channel
            .basic_consume(
                &config.queue,
                &config.consumer_tag,
                BasicConsumeOptions {
                    no_ack: config.auto_ack,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
        {
            Ok(c) => c,
            Err(e) => {
                error!(error = %e, "AMQP consume failed");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        debug!(queue = %config.queue, "AMQP consumer started");

        loop {
            if shutdown_rx.try_recv().is_ok() {
                let _ = sender.send_async(Err(Error::EndOfInput)).await;
                return;
            }

            match tokio::time::timeout(std::time::Duration::from_secs(1), consumer.next()).await {
                Ok(Some(Ok(delivery))) => {
                    let msg = Message {
                        bytes: delivery.data.clone(),
                        ..Default::default()
                    };
                    let info = if !config.auto_ack {
                        Some(DeliveryInfo {
                            delivery_tag: delivery.delivery_tag,
                            channel: channel.clone(),
                        })
                    } else {
                        None
                    };
                    if sender.send_async(Ok((msg, info))).await.is_err() {
                        return;
                    }
                }
                Ok(Some(Err(e))) => {
                    warn!(error = %e, "AMQP delivery error");
                    break; // Reconnect
                }
                Ok(None) => break, // Stream ended, reconnect
                Err(_) => {} // Timeout
            }
        }
    }
}

pub struct AmqpInput {
    receiver: Receiver<Result<(Message, Option<DeliveryInfo>), Error>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    pending_ack: Option<DeliveryInfo>,
}

impl AmqpInput {
    pub fn new(config: AmqpInputConfig) -> Result<Self, Error> {
        let (sender, receiver) = bounded(CHANNEL_BUFFER);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        tokio::spawn(amqp_reader_task(config, sender, shutdown_rx));

        Ok(Self {
            receiver,
            shutdown_tx: Some(shutdown_tx),
            pending_ack: None,
        })
    }
}

#[async_trait]
impl Input for AmqpInput {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        // Ack previous message if exists
        if let Some(info) = self.pending_ack.take() {
            let _ = info.channel.basic_ack(info.delivery_tag, BasicAckOptions::default()).await;
        }

        match self.receiver.recv_async().await {
            Ok(Ok((msg, info))) => {
                self.pending_ack = info;
                Ok((msg, None))
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::EndOfInput),
        }
    }
}

#[async_trait]
impl Closer for AmqpInput {
    async fn close(&mut self) -> Result<(), Error> {
        // Nack pending message on close
        if let Some(info) = self.pending_ack.take() {
            let _ = info.channel.basic_nack(
                info.delivery_tag,
                BasicNackOptions { requeue: true, ..Default::default() },
            ).await;
        }
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        debug!("AMQP input closed");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_amqp_input(conf: Value) -> Result<ExecutionType, Error> {
    let config: AmqpInputConfig = serde_yaml::from_value(conf)?;

    if config.url.is_empty() {
        return Err(Error::ConfigFailedValidation("url is required".into()));
    }
    if config.queue.is_empty() {
        return Err(Error::ConfigFailedValidation("queue is required".into()));
    }

    Ok(ExecutionType::Input(Box::new(AmqpInput::new(config)?)))
}

pub(crate) fn register_amqp() -> Result<(), Error> {
    let config = r#"type: object
required:
  - url
  - queue
properties:
  url:
    type: string
    description: "AMQP connection URL"
  queue:
    type: string
    description: "Queue name to consume from"
  consumer_tag:
    type: string
    default: "fiddler"
    description: "Consumer tag"
  prefetch_count:
    type: integer
    default: 100
    description: "QoS prefetch count"
  auto_ack:
    type: boolean
    default: false
    description: "Auto-acknowledge messages"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;
    register_plugin("amqp".into(), ItemType::Input, conf_spec, create_amqp_input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
url: "amqp://guest:guest@localhost:5672/%2f"
queue: "events"
prefetch_count: 50
auto_ack: true
"#;
        let config: AmqpInputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.queue, "events");
        assert_eq!(config.prefetch_count, 50);
        assert!(config.auto_ack);
    }

    #[test]
    fn test_config_defaults() {
        let yaml = r#"
url: "amqp://localhost"
queue: "test"
"#;
        let config: AmqpInputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.prefetch_count, 100);
        assert!(!config.auto_ack);
        assert_eq!(config.consumer_tag, "fiddler");
    }

    #[test]
    fn test_register() {
        let result = register_amqp();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
