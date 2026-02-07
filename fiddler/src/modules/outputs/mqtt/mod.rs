//! MQTT output module for publishing to topics.
//!
//! # Configuration
//!
//! ```yaml
//! output:
//!   mqtt:
//!     broker: "tcp://localhost:1883"      # Required
//!     client_id: "fiddler_output"         # Optional
//!     topic: "fiddler/output"             # Required
//!     qos: 1                               # Optional: 0, 1, or 2 (default: 1)
//!     retain: false                       # Optional (default: false)
//!     username: "user"                    # Optional
//!     password: "pass"                    # Optional
//! ```

use crate::config::{register_plugin, ConfigSpec, ExecutionType, ItemType};
use crate::{Closer, Error, Message, Output};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use serde::Deserialize;
use serde_yaml::Value;
use tracing::debug;
use uuid::Uuid;

const DEFAULT_QOS: u8 = 1;

#[derive(Deserialize, Clone)]
pub struct MqttOutputConfig {
    pub broker: String,
    #[serde(default = "default_client_id")]
    pub client_id: String,
    pub topic: String,
    #[serde(default = "default_qos")]
    pub qos: u8,
    #[serde(default)]
    pub retain: bool,
    pub username: Option<String>,
    pub password: Option<String>,
}

fn default_client_id() -> String {
    format!("fiddler_{}", Uuid::new_v4())
}

fn default_qos() -> u8 {
    DEFAULT_QOS
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

pub struct MqttOutput {
    client: AsyncClient,
    _eventloop: tokio::task::JoinHandle<()>,
    topic: String,
    qos: QoS,
    retain: bool,
}

impl MqttOutput {
    pub fn new(config: MqttOutputConfig) -> Result<Self, Error> {
        let mut opts = parse_broker(&config.broker, &config.client_id);

        if let (Some(user), Some(pass)) = (&config.username, &config.password) {
            opts.set_credentials(user, pass);
        }

        let (client, eventloop) = AsyncClient::new(opts, 10);

        // Spawn eventloop to handle acks/reconnections
        let handle = tokio::spawn(run_eventloop(eventloop));

        debug!(topic = %config.topic, "MQTT output initialized");

        Ok(Self {
            client,
            _eventloop: handle,
            topic: config.topic,
            qos: to_qos(config.qos),
            retain: config.retain,
        })
    }
}

async fn run_eventloop(mut eventloop: EventLoop) {
    loop {
        if eventloop.poll().await.is_err() {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
}

#[async_trait]
impl Output for MqttOutput {
    async fn write(&mut self, message: Message) -> Result<(), Error> {
        self.client
            .publish(&self.topic, self.qos, self.retain, message.bytes)
            .await
            .map_err(|e| Error::OutputError(format!("MQTT publish failed: {}", e)))
    }
}

#[async_trait]
impl Closer for MqttOutput {
    async fn close(&mut self) -> Result<(), Error> {
        let _ = self.client.disconnect().await;
        debug!("MQTT output closed");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_mqtt_output(conf: Value) -> Result<ExecutionType, Error> {
    let config: MqttOutputConfig = serde_yaml::from_value(conf)?;

    if config.broker.is_empty() {
        return Err(Error::ConfigFailedValidation("broker is required".into()));
    }
    if config.topic.is_empty() {
        return Err(Error::ConfigFailedValidation("topic is required".into()));
    }
    if config.qos > 2 {
        return Err(Error::ConfigFailedValidation(
            "qos must be 0, 1, or 2".into(),
        ));
    }

    Ok(ExecutionType::Output(Box::new(MqttOutput::new(config)?)))
}

pub(crate) fn register_mqtt() -> Result<(), Error> {
    let config = r#"type: object
required:
  - broker
  - topic
properties:
  broker:
    type: string
    description: "MQTT broker URL"
  client_id:
    type: string
    description: "Client ID"
  topic:
    type: string
    description: "Topic to publish to"
  qos:
    type: integer
    enum: [0, 1, 2]
    default: 1
    description: "Quality of Service"
  retain:
    type: boolean
    default: false
    description: "Retain flag"
  username:
    type: string
  password:
    type: string
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;
    register_plugin(
        "mqtt".into(),
        ItemType::Output,
        conf_spec,
        create_mqtt_output,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
broker: "tcp://localhost:1883"
topic: "output/topic"
qos: 2
retain: true
"#;
        let config: MqttOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.broker, "tcp://localhost:1883");
        assert_eq!(config.topic, "output/topic");
        assert_eq!(config.qos, 2);
        assert!(config.retain);
    }

    #[test]
    fn test_config_defaults() {
        let yaml = r#"
broker: "tcp://localhost:1883"
topic: "test"
"#;
        let config: MqttOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.qos, 1);
        assert!(!config.retain);
    }

    #[test]
    fn test_register() {
        let result = register_mqtt();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
