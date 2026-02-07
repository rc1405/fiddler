//! ZeroMQ input module for receiving messages.
//!
//! # Configuration
//!
//! ```yaml
//! input:
//!   zeromq:
//!     socket_type: "pull"                 # Required: pull, sub
//!     bind: "tcp://*:5555"                # Optional: bind address
//!     connect:                            # Optional: connect addresses
//!       - "tcp://localhost:5555"
//!     subscribe:                          # Required for sub: topic filters
//!       - ""                              # Empty = all messages
//! ```

use crate::config::{register_plugin, ConfigSpec, ExecutionType, ItemType};
use crate::{CallbackChan, Closer, Error, Input, Message};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, Sender};
use serde::Deserialize;
use serde_yaml::Value;
use tokio::sync::oneshot;
use tracing::{debug, error};
use zeromq::{PullSocket, Socket, SocketRecv, SubSocket};

const CHANNEL_BUFFER: usize = 100;

#[derive(Deserialize, Clone)]
pub struct ZmqInputConfig {
    pub socket_type: String,
    pub bind: Option<String>,
    #[serde(default)]
    pub connect: Vec<String>,
    #[serde(default)]
    pub subscribe: Vec<String>,
}

async fn pull_reader_task(
    config: ZmqInputConfig,
    sender: Sender<Result<Message, Error>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut socket = PullSocket::new();

    if let Some(ref addr) = config.bind {
        if let Err(e) = socket.bind(addr).await {
            let _ = sender.send_async(Err(Error::ExecutionError(format!("Bind failed: {}", e)))).await;
            return;
        }
    }
    for addr in &config.connect {
        if let Err(e) = socket.connect(addr).await {
            error!(error = %e, addr = %addr, "Connect failed");
        }
    }

    debug!(socket_type = "pull", "ZeroMQ input started");

    loop {
        if shutdown_rx.try_recv().is_ok() {
            let _ = sender.send_async(Err(Error::EndOfInput)).await;
            return;
        }

        match tokio::time::timeout(std::time::Duration::from_secs(1), socket.recv()).await {
            Ok(Ok(msg)) => {
                let bytes: Vec<u8> = msg.into_vec().into_iter().flat_map(|f| f.to_vec()).collect();
                let message = Message { bytes, ..Default::default() };
                if sender.send_async(Ok(message)).await.is_err() {
                    return;
                }
            }
            Ok(Err(e)) => {
                error!(error = %e, "ZeroMQ recv error");
            }
            Err(_) => {} // Timeout
        }
    }
}

async fn sub_reader_task(
    config: ZmqInputConfig,
    sender: Sender<Result<Message, Error>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut socket = SubSocket::new();

    if let Some(ref addr) = config.bind {
        if let Err(e) = socket.bind(addr).await {
            let _ = sender.send_async(Err(Error::ExecutionError(format!("Bind failed: {}", e)))).await;
            return;
        }
    }
    for addr in &config.connect {
        if let Err(e) = socket.connect(addr).await {
            error!(error = %e, addr = %addr, "Connect failed");
        }
    }

    // Subscribe to topics
    for topic in &config.subscribe {
        if let Err(e) = socket.subscribe(topic).await {
            error!(error = %e, topic = %topic, "Subscribe failed");
        }
    }

    debug!(socket_type = "sub", topics = ?config.subscribe, "ZeroMQ input started");

    loop {
        if shutdown_rx.try_recv().is_ok() {
            let _ = sender.send_async(Err(Error::EndOfInput)).await;
            return;
        }

        match tokio::time::timeout(std::time::Duration::from_secs(1), socket.recv()).await {
            Ok(Ok(msg)) => {
                let bytes: Vec<u8> = msg.into_vec().into_iter().flat_map(|f| f.to_vec()).collect();
                let message = Message { bytes, ..Default::default() };
                if sender.send_async(Ok(message)).await.is_err() {
                    return;
                }
            }
            Ok(Err(e)) => {
                error!(error = %e, "ZeroMQ recv error");
            }
            Err(_) => {} // Timeout
        }
    }
}

pub struct ZmqInput {
    receiver: Receiver<Result<Message, Error>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl ZmqInput {
    pub fn new(config: ZmqInputConfig) -> Result<Self, Error> {
        let (sender, receiver) = bounded(CHANNEL_BUFFER);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        match config.socket_type.as_str() {
            "pull" => tokio::spawn(pull_reader_task(config, sender, shutdown_rx)),
            "sub" => tokio::spawn(sub_reader_task(config, sender, shutdown_rx)),
            _ => return Err(Error::ConfigFailedValidation("socket_type must be 'pull' or 'sub'".into())),
        };

        Ok(Self { receiver, shutdown_tx: Some(shutdown_tx) })
    }
}

#[async_trait]
impl Input for ZmqInput {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        match self.receiver.recv_async().await {
            Ok(Ok(msg)) => Ok((msg, None)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::EndOfInput),
        }
    }
}

#[async_trait]
impl Closer for ZmqInput {
    async fn close(&mut self) -> Result<(), Error> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        debug!("ZeroMQ input closed");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_zeromq_input(conf: Value) -> Result<ExecutionType, Error> {
    let config: ZmqInputConfig = serde_yaml::from_value(conf)?;

    if config.bind.is_none() && config.connect.is_empty() {
        return Err(Error::ConfigFailedValidation("bind or connect is required".into()));
    }
    if config.socket_type == "sub" && config.subscribe.is_empty() {
        return Err(Error::ConfigFailedValidation("subscribe is required for sub socket".into()));
    }

    Ok(ExecutionType::Input(Box::new(ZmqInput::new(config)?)))
}

pub(crate) fn register_zeromq() -> Result<(), Error> {
    let config = r#"type: object
required:
  - socket_type
properties:
  socket_type:
    type: string
    enum: ["pull", "sub"]
    description: "Socket type"
  bind:
    type: string
    description: "Bind address"
  connect:
    type: array
    items:
      type: string
    description: "Connect addresses"
  subscribe:
    type: array
    items:
      type: string
    description: "Subscribe topics (for sub socket)"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;
    register_plugin("zeromq".into(), ItemType::Input, conf_spec, create_zeromq_input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
socket_type: "pull"
bind: "tcp://*:5555"
"#;
        let config: ZmqInputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.socket_type, "pull");
        assert_eq!(config.bind, Some("tcp://*:5555".to_string()));
    }

    #[test]
    fn test_config_sub() {
        let yaml = r#"
socket_type: "sub"
connect:
  - "tcp://localhost:5555"
subscribe:
  - "topic1"
  - ""
"#;
        let config: ZmqInputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.socket_type, "sub");
        assert_eq!(config.subscribe.len(), 2);
    }

    #[test]
    fn test_register() {
        let result = register_zeromq();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
