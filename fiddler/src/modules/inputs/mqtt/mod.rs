//! MQTT input module for subscribing to topics.
//!
//! # Configuration
//!
//! ```yaml
//! input:
//!   mqtt:
//!     broker: "tcp://localhost:1883"      # Required
//!     client_id: "fiddler_input"          # Optional (auto-generated if not set)
//!     topics:                              # Required
//!       - "events/+"
//!       - "logs/#"
//!     qos: 1                               # Optional: 0, 1, or 2 (default: 1)
//!     username: "user"                    # Optional
//!     password: "pass"                    # Optional
//!     keep_alive_secs: 60                 # Optional (default: 60)
//! ```

use crate::config::{register_plugin, ConfigSpec, ExecutionType, ItemType};
use crate::{CallbackChan, Closer, Error, Input, Message};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, Sender};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde::Deserialize;
use serde_yaml::Value;
use tokio::sync::oneshot;
use tracing::{debug, error, warn};
use uuid::Uuid;

const DEFAULT_QOS: u8 = 1;
const DEFAULT_KEEP_ALIVE: u64 = 60;
const CHANNEL_BUFFER: usize = 100;

#[derive(Deserialize, Clone)]
pub struct MqttInputConfig {
    pub broker: String,
    #[serde(default = "default_client_id")]
    pub client_id: String,
    pub topics: Vec<String>,
    #[serde(default = "default_qos")]
    pub qos: u8,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default = "default_keep_alive")]
    pub keep_alive_secs: u64,
}

fn default_client_id() -> String {
    format!("fiddler_{}", Uuid::new_v4())
}

fn default_qos() -> u8 {
    DEFAULT_QOS
}

fn default_keep_alive() -> u64 {
    DEFAULT_KEEP_ALIVE
}

fn to_qos(qos: u8) -> QoS {
    match qos {
        0 => QoS::AtMostOnce,
        1 => QoS::AtLeastOnce,
        _ => QoS::ExactlyOnce,
    }
}

/// Parse broker URL like "tcp://host:port" or "host:port" or just "host"
fn parse_broker(broker: &str, client_id: &str) -> MqttOptions {
    let cleaned = broker
        .trim_start_matches("tcp://")
        .trim_start_matches("mqtt://");
    let (host, port) = if let Some((h, p)) = cleaned.rsplit_once(':') {
        (h, p.parse().unwrap_or(1883))
    } else {
        (cleaned, 1883)
    };
    MqttOptions::new(client_id, host, port)
}

async fn mqtt_reader_task(
    config: MqttInputConfig,
    sender: Sender<Result<Message, Error>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut opts = parse_broker(&config.broker, &config.client_id);
    opts.set_keep_alive(std::time::Duration::from_secs(config.keep_alive_secs));

    if let (Some(user), Some(pass)) = (&config.username, &config.password) {
        opts.set_credentials(user, pass);
    }

    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    let qos = to_qos(config.qos);

    // Subscribe to all topics
    for topic in &config.topics {
        if let Err(e) = client.subscribe(topic, qos).await {
            error!(error = %e, topic = %topic, "Failed to subscribe");
        }
    }

    debug!(topics = ?config.topics, "MQTT input subscribed");

    loop {
        if shutdown_rx.try_recv().is_ok() {
            debug!("MQTT reader received shutdown");
            let _ = sender.send_async(Err(Error::EndOfInput)).await;
            return;
        }

        match tokio::time::timeout(
            std::time::Duration::from_secs(1),
            eventloop.poll(),
        ).await {
            Ok(Ok(Event::Incoming(Packet::Publish(publish)))) => {
                let msg = Message {
                    bytes: publish.payload.to_vec(),
                    ..Default::default()
                };
                if sender.send_async(Ok(msg)).await.is_err() {
                    return;
                }
            }
            Ok(Ok(_)) => {} // Other events, ignore
            Ok(Err(e)) => {
                warn!(error = %e, "MQTT connection error, reconnecting...");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            Err(_) => {} // Timeout, continue
        }
    }
}

pub struct MqttInput {
    receiver: Receiver<Result<Message, Error>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl MqttInput {
    pub fn new(config: MqttInputConfig) -> Result<Self, Error> {
        let (sender, receiver) = bounded(CHANNEL_BUFFER);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        tokio::spawn(mqtt_reader_task(config, sender, shutdown_rx));

        Ok(Self {
            receiver,
            shutdown_tx: Some(shutdown_tx),
        })
    }
}

#[async_trait]
impl Input for MqttInput {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        match self.receiver.recv_async().await {
            Ok(Ok(msg)) => Ok((msg, None)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::EndOfInput),
        }
    }
}

#[async_trait]
impl Closer for MqttInput {
    async fn close(&mut self) -> Result<(), Error> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        debug!("MQTT input closed");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_mqtt_input(conf: Value) -> Result<ExecutionType, Error> {
    let config: MqttInputConfig = serde_yaml::from_value(conf)?;

    if config.broker.is_empty() {
        return Err(Error::ConfigFailedValidation("broker is required".into()));
    }
    if config.topics.is_empty() {
        return Err(Error::ConfigFailedValidation("topics is required".into()));
    }
    if config.qos > 2 {
        return Err(Error::ConfigFailedValidation("qos must be 0, 1, or 2".into()));
    }

    Ok(ExecutionType::Input(Box::new(MqttInput::new(config)?)))
}

pub(crate) fn register_mqtt() -> Result<(), Error> {
    let config = r#"type: object
required:
  - broker
  - topics
properties:
  broker:
    type: string
    description: "MQTT broker URL (tcp://host:port)"
  client_id:
    type: string
    description: "Client ID (auto-generated if not set)"
  topics:
    type: array
    items:
      type: string
    description: "Topics to subscribe to"
  qos:
    type: integer
    enum: [0, 1, 2]
    default: 1
    description: "Quality of Service level"
  username:
    type: string
    description: "Username for authentication"
  password:
    type: string
    description: "Password for authentication"
  keep_alive_secs:
    type: integer
    default: 60
    description: "Keep-alive interval in seconds"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;
    register_plugin("mqtt".into(), ItemType::Input, conf_spec, create_mqtt_input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
broker: "tcp://localhost:1883"
topics:
  - "events/+"
  - "logs/#"
qos: 2
username: "user"
password: "pass"
"#;
        let config: MqttInputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.broker, "tcp://localhost:1883");
        assert_eq!(config.topics.len(), 2);
        assert_eq!(config.qos, 2);
    }

    #[test]
    fn test_config_defaults() {
        let yaml = r#"
broker: "tcp://localhost:1883"
topics:
  - "test"
"#;
        let config: MqttInputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.qos, 1);
        assert_eq!(config.keep_alive_secs, 60);
    }

    #[test]
    fn test_register() {
        let result = register_mqtt();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
