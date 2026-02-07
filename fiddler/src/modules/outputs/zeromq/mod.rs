//! ZeroMQ output module for sending messages.
//!
//! # Configuration
//!
//! ```yaml
//! output:
//!   zeromq:
//!     socket_type: "push"                 # Required: push, pub
//!     bind: "tcp://*:5555"                # Optional: bind address
//!     connect:                            # Optional: connect addresses
//!       - "tcp://localhost:5555"
//! ```

use crate::config::{register_plugin, ConfigSpec, ExecutionType, ItemType};
use crate::{Closer, Error, Message, Output};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use serde::Deserialize;
use serde_yaml::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error};
use zeromq::{PubSocket, PushSocket, Socket, SocketSend, ZmqMessage};

#[derive(Deserialize, Clone)]
pub struct ZmqOutputConfig {
    pub socket_type: String,
    pub bind: Option<String>,
    #[serde(default)]
    pub connect: Vec<String>,
}

enum ZmqSocket {
    Push(PushSocket),
    Pub(PubSocket),
}

impl ZmqSocket {
    async fn send(&mut self, msg: ZmqMessage) -> Result<(), zeromq::ZmqError> {
        match self {
            ZmqSocket::Push(s) => s.send(msg).await,
            ZmqSocket::Pub(s) => s.send(msg).await,
        }
    }
}

pub struct ZmqOutput {
    socket: Arc<Mutex<ZmqSocket>>,
}

impl ZmqOutput {
    pub async fn new(config: ZmqOutputConfig) -> Result<Self, Error> {
        let socket = match config.socket_type.as_str() {
            "push" => {
                let mut s = PushSocket::new();
                if let Some(ref addr) = config.bind {
                    s.bind(addr)
                        .await
                        .map_err(|e| Error::ExecutionError(format!("Bind failed: {}", e)))?;
                }
                for addr in &config.connect {
                    if let Err(e) = s.connect(addr).await {
                        error!(error = %e, addr = %addr, "Connect failed");
                    }
                }
                ZmqSocket::Push(s)
            }
            "pub" => {
                let mut s = PubSocket::new();
                if let Some(ref addr) = config.bind {
                    s.bind(addr)
                        .await
                        .map_err(|e| Error::ExecutionError(format!("Bind failed: {}", e)))?;
                }
                for addr in &config.connect {
                    if let Err(e) = s.connect(addr).await {
                        error!(error = %e, addr = %addr, "Connect failed");
                    }
                }
                ZmqSocket::Pub(s)
            }
            _ => {
                return Err(Error::ConfigFailedValidation(
                    "socket_type must be 'push' or 'pub'".into(),
                ))
            }
        };

        debug!(socket_type = %config.socket_type, "ZeroMQ output initialized");

        Ok(Self {
            socket: Arc::new(Mutex::new(socket)),
        })
    }
}

#[async_trait]
impl Output for ZmqOutput {
    async fn write(&mut self, message: Message) -> Result<(), Error> {
        let msg = ZmqMessage::from(message.bytes);
        self.socket
            .lock()
            .await
            .send(msg)
            .await
            .map_err(|e| Error::OutputError(format!("ZeroMQ send failed: {}", e)))
    }
}

#[async_trait]
impl Closer for ZmqOutput {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("ZeroMQ output closed");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_zeromq_output(conf: Value) -> Result<ExecutionType, Error> {
    let config: ZmqOutputConfig = serde_yaml::from_value(conf)?;

    if config.bind.is_none() && config.connect.is_empty() {
        return Err(Error::ConfigFailedValidation(
            "bind or connect is required".into(),
        ));
    }

    Ok(ExecutionType::Output(Box::new(
        ZmqOutput::new(config).await?,
    )))
}

pub(crate) fn register_zeromq() -> Result<(), Error> {
    let config = r#"type: object
required:
  - socket_type
properties:
  socket_type:
    type: string
    enum: ["push", "pub"]
    description: "Socket type"
  bind:
    type: string
    description: "Bind address"
  connect:
    type: array
    items:
      type: string
    description: "Connect addresses"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;
    register_plugin(
        "zeromq".into(),
        ItemType::Output,
        conf_spec,
        create_zeromq_output,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
socket_type: "push"
bind: "tcp://*:5556"
"#;
        let config: ZmqOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.socket_type, "push");
        assert_eq!(config.bind, Some("tcp://*:5556".to_string()));
    }

    #[test]
    fn test_config_connect() {
        let yaml = r#"
socket_type: "pub"
connect:
  - "tcp://localhost:5555"
  - "tcp://localhost:5556"
"#;
        let config: ZmqOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.socket_type, "pub");
        assert_eq!(config.connect.len(), 2);
    }

    #[test]
    fn test_register() {
        let result = register_zeromq();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
