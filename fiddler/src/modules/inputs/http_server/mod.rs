//! HTTP Server input module for receiving JSON data via HTTP POST requests.
//!
//! This module provides an HTTP server that accepts JSON objects and feeds them
//! into the processing pipeline.
//!
//! # Configuration
//!
//! ```yaml
//! input:
//!   http_server:
//!     address: "0.0.0.0"                  # Optional: Bind address (default: "0.0.0.0")
//!     port: 8080                          # Optional: Port number (default: 8080)
//!     path: "/ingest"                     # Optional: Endpoint path (default: "/ingest")
//!     max_body_size: 10485760             # Optional: Max body size in bytes (default: 10MB)
//!     acknowledgment: true                # Optional: Wait for processing before responding (default: true)
//!     cors_enabled: false                 # Optional: Enable CORS (default: false)
//! ```
//!
//! # Endpoints
//!
//! ## POST `{path}`
//!
//! Accepts a JSON object or array of JSON objects. Each object becomes a separate
//! message in the pipeline.
//!
//! **Request:**
//! - Content-Type: application/json
//! - Body: JSON object or array of objects
//!
//! **Response:**
//! - 200 OK: Message(s) successfully processed (if acknowledgment=true) or accepted
//! - 400 Bad Request: Invalid JSON
//! - 413 Payload Too Large: Body exceeds max_body_size
//! - 500 Internal Server Error: Processing failed
//!
//! ## GET `/health`
//!
//! Health check endpoint.
//!
//! **Response:**
//! - 200 OK: `{"status": "healthy"}`
//!
//! # Example
//!
//! ```yaml
//! input:
//!   http_server:
//!     port: 9000
//!     path: "/events"
//!     acknowledgment: true
//!
//! processors:
//!   - jmespath:
//!       expression: "@"
//!
//! output:
//!   clickhouse:
//!     url: "http://localhost:8123"
//!     table: "events"
//! ```
//!
//! Then send data:
//! ```bash
//! curl -X POST http://localhost:9000/events \
//!   -H "Content-Type: application/json" \
//!   -d '{"event": "click", "user_id": 123}'
//! ```

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::{new_callback_chan, CallbackChan, Status};
use crate::{Closer, Error, Input};
use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, Sender};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use serde_yaml::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::oneshot;
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, error, info, warn};

const DEFAULT_ADDRESS: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 8080;
const DEFAULT_PATH: &str = "/ingest";
const DEFAULT_MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10MB

/// HTTP Server input configuration.
#[derive(Deserialize, Clone)]
pub struct HttpServerConfig {
    /// Bind address (default: "0.0.0.0").
    #[serde(default = "default_address")]
    pub address: String,
    /// Port number (default: 8080).
    #[serde(default = "default_port")]
    pub port: u16,
    /// Endpoint path for ingestion (default: "/ingest").
    #[serde(default = "default_path")]
    pub path: String,
    /// Maximum body size in bytes (default: 10MB).
    #[serde(default = "default_max_body_size")]
    pub max_body_size: usize,
    /// Wait for processing acknowledgment before responding (default: true).
    #[serde(default = "default_acknowledgment")]
    pub acknowledgment: bool,
    /// Enable CORS headers (default: false).
    #[serde(default)]
    pub cors_enabled: bool,
}

fn default_address() -> String {
    DEFAULT_ADDRESS.to_string()
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_path() -> String {
    DEFAULT_PATH.to_string()
}

fn default_max_body_size() -> usize {
    DEFAULT_MAX_BODY_SIZE
}

fn default_acknowledgment() -> bool {
    true
}

/// Shared state for the HTTP server handlers.
struct AppState {
    sender: Sender<Result<(Message, Option<CallbackChan>), Error>>,
    max_body_size: usize,
    acknowledgment: bool,
}

/// Health check handler.
async fn health_handler() -> impl IntoResponse {
    Json(json!({"status": "healthy"}))
}

/// Ingest handler for receiving JSON data.
async fn ingest_handler(State(state): State<Arc<AppState>>, body: Bytes) -> impl IntoResponse {
    // Check body size
    if body.len() > state.max_body_size {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "Payload too large",
                "max_size": state.max_body_size
            })),
        );
    }

    // Parse JSON
    let json_value: JsonValue = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "Invalid JSON",
                    "details": e.to_string()
                })),
            );
        }
    };

    // Handle both single objects and arrays
    let messages: Vec<JsonValue> = if json_value.is_array() {
        json_value.as_array().unwrap().clone()
    } else if json_value.is_object() {
        vec![json_value]
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Expected JSON object or array of objects"
            })),
        );
    };

    if messages.is_empty() {
        return (
            StatusCode::OK,
            Json(json!({
                "accepted": 0,
                "message": "No messages to process"
            })),
        );
    }

    let message_count = messages.len();
    let mut errors: Vec<String> = Vec::new();

    for (idx, msg_json) in messages.into_iter().enumerate() {
        if !msg_json.is_object() {
            warn!(index = idx, "Skipping non-object JSON element");
            errors.push(format!("Element {} is not a JSON object", idx));
            continue;
        }

        // Serialize back to bytes
        let bytes = match serde_json::to_vec(&msg_json) {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("Failed to serialize element {}: {}", idx, e));
                continue;
            }
        };

        let message = Message {
            bytes,
            ..Default::default()
        };

        if state.acknowledgment {
            // Create callback channel for acknowledgment
            let (callback_tx, callback_rx) = new_callback_chan();
            let (response_tx, response_rx) = oneshot::channel();

            // Spawn task to wait for callback and send response
            let response_tx_clone = response_tx;
            tokio::spawn(async move {
                match callback_rx.await {
                    Ok(Status::Processed) => {
                        let _ = response_tx_clone.send(Ok(()));
                    }
                    Ok(Status::Errored(errs)) => {
                        let _ = response_tx_clone.send(Err(errs.join(", ")));
                    }
                    Err(_) => {
                        let _ = response_tx_clone.send(Err("Callback channel closed".to_string()));
                    }
                }
            });

            // Send message to pipeline
            if let Err(e) = state
                .sender
                .send_async(Ok((message, Some(callback_tx))))
                .await
            {
                errors.push(format!("Failed to send message {}: {}", idx, e));
                continue;
            }

            // Wait for acknowledgment
            match response_rx.await {
                Ok(Ok(())) => {
                    // Success
                }
                Ok(Err(e)) => {
                    errors.push(format!("Processing failed for message {}: {}", idx, e));
                }
                Err(_) => {
                    errors.push(format!("Response channel closed for message {}", idx));
                }
            }
        } else {
            // Fire and forget mode
            if let Err(e) = state.sender.send_async(Ok((message, None))).await {
                errors.push(format!("Failed to send message {}: {}", idx, e));
            }
        }
    }

    if errors.is_empty() {
        (
            StatusCode::OK,
            Json(json!({
                "accepted": message_count,
                "processed": message_count
            })),
        )
    } else if errors.len() == message_count {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "All messages failed",
                "errors": errors
            })),
        )
    } else {
        (
            StatusCode::OK,
            Json(json!({
                "accepted": message_count,
                "processed": message_count - errors.len(),
                "errors": errors
            })),
        )
    }
}

/// HTTP Server input.
///
/// Starts an HTTP server that accepts JSON objects via POST requests
/// and feeds them into the processing pipeline.
pub struct HttpServerInput {
    receiver: Receiver<Result<(Message, Option<CallbackChan>), Error>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl HttpServerInput {
    /// Creates a new HTTP server input from configuration.
    pub fn new(config: HttpServerConfig) -> Result<Self, Error> {
        let (sender, receiver) = bounded(1000);

        let state = Arc::new(AppState {
            sender,
            max_body_size: config.max_body_size,
            acknowledgment: config.acknowledgment,
        });

        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let path = config.path.clone();
        let address = config.address.clone();
        let port = config.port;
        let cors_enabled = config.cors_enabled;

        // Build router
        let mut app = Router::new()
            .route(&path, post(ingest_handler))
            .route("/health", get(health_handler))
            .with_state(state);

        // Add CORS if enabled
        if cors_enabled {
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any);
            app = app.layer(cors);
        }

        // Spawn server task
        tokio::spawn(async move {
            let addr: SocketAddr = format!("{}:{}", address, port)
                .parse()
                .expect("Invalid address format");

            info!(address = %addr, path = %path, "Starting HTTP server");

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!(error = %e, "Failed to bind HTTP server");
                    return;
                }
            };

            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                    info!("HTTP server shutting down");
                })
                .await
                .unwrap_or_else(|e| error!(error = %e, "HTTP server error"));
        });

        debug!(
            address = %config.address,
            port = config.port,
            path = %config.path,
            "HTTP server input initialized"
        );

        Ok(Self {
            receiver,
            shutdown_tx: Some(shutdown_tx),
        })
    }
}

#[async_trait]
impl Input for HttpServerInput {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        match self.receiver.recv_async().await {
            Ok(result) => result,
            Err(_) => Err(Error::EndOfInput),
        }
    }
}

#[async_trait]
impl Closer for HttpServerInput {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("HTTP server input closing");
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_http_server(conf: Value) -> Result<ExecutionType, Error> {
    let config: HttpServerConfig = serde_yaml::from_value(conf)?;

    // Validate path starts with /
    if !config.path.starts_with('/') {
        return Err(Error::ConfigFailedValidation(
            "path must start with '/'".into(),
        ));
    }

    Ok(ExecutionType::Input(Box::new(HttpServerInput::new(
        config,
    )?)))
}

/// Registers the HTTP server input plugin.
pub(crate) fn register_http_server() -> Result<(), Error> {
    let config = r#"type: object
properties:
  address:
    type: string
    default: "0.0.0.0"
    description: "Bind address"
  port:
    type: integer
    default: 8080
    description: "Port number"
  path:
    type: string
    default: "/ingest"
    description: "Endpoint path for ingestion"
  max_body_size:
    type: integer
    default: 10485760
    description: "Maximum body size in bytes"
  acknowledgment:
    type: boolean
    default: true
    description: "Wait for processing acknowledgment before responding"
  cors_enabled:
    type: boolean
    default: false
    description: "Enable CORS headers"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "http_server".into(),
        ItemType::Input,
        conf_spec,
        create_http_server,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        assert_eq!(default_address(), "0.0.0.0");
        assert_eq!(default_port(), 8080);
        assert_eq!(default_path(), "/ingest");
        assert_eq!(default_max_body_size(), 10 * 1024 * 1024);
        assert!(default_acknowledgment());
    }

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
address: "127.0.0.1"
port: 9000
path: "/events"
max_body_size: 1048576
acknowledgment: false
cors_enabled: true
"#;
        let config: HttpServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.address, "127.0.0.1");
        assert_eq!(config.port, 9000);
        assert_eq!(config.path, "/events");
        assert_eq!(config.max_body_size, 1048576);
        assert!(!config.acknowledgment);
        assert!(config.cors_enabled);
    }

    #[test]
    fn test_config_defaults() {
        let yaml = "{}";
        let config: HttpServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.address, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.path, "/ingest");
        assert_eq!(config.max_body_size, 10 * 1024 * 1024);
        assert!(config.acknowledgment);
        assert!(!config.cors_enabled);
    }

    #[test]
    fn test_register_http_server() {
        let result = register_http_server();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
