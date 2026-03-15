//! HTTP Server input module for receiving JSON data via HTTP POST requests.
//!
//! This module provides an HTTP server that accepts JSON objects and feeds them
//! into the processing pipeline, with optional authentication.
//!
//! # Configuration
//!
//! ```yaml
//! input:
//!   http_server:
//!     address: "0.0.0.0"                  # Optional: Bind address (default: "0.0.0.0")
//!     port: 8080                          # Optional: Port number (default: 8080)
//!     path: "/ingest"                     # Optional: Endpoint path (default: "/ingest"), supports :param syntax
//!     max_body_size: 10485760             # Optional: Max body size in bytes (default: 10MB)
//!     acknowledgment: true                # Optional: Wait for processing before responding (default: true)
//!     cors_enabled: false                 # Optional: Enable CORS (default: false)
//!     auth:                               # Optional: Authentication for incoming requests
//!       type: basic
//!       username: admin
//!       password: secret
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
use crate::modules::tls::ServerTlsConfig;
use crate::Message;
use crate::{new_callback_chan, CallbackChan, Status};
use crate::{Closer, Error, InputBatch, MessageBatch};
use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use base64::{prelude::BASE64_STANDARD, Engine};
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, Sender};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use serde_yaml::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::oneshot;
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, error, info, warn};

const DEFAULT_ADDRESS: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 8080;
const DEFAULT_PATH: &str = "/ingest";
const DEFAULT_MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10MB

/// Authentication configuration for incoming requests.
#[derive(Deserialize, Clone)]
#[serde(tag = "type")]
pub enum AuthConfig {
    /// HTTP Basic authentication.
    #[serde(rename = "basic")]
    Basic {
        /// Username.
        username: String,
        /// Password.
        password: String,
    },
    /// Bearer token authentication.
    #[serde(rename = "bearer")]
    Bearer {
        /// Token.
        token: String,
    },
}

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
    /// TLS configuration for HTTPS support.
    pub tls: Option<ServerTlsConfig>,
    /// Authentication for incoming requests.
    pub auth: Option<AuthConfig>,
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
    sender: Sender<(Vec<Message>, Option<CallbackChan>)>,
    max_body_size: usize,
    acknowledgment: bool,
    auth: Option<AuthConfig>,
}

/// Health check handler.
async fn health_handler() -> impl IntoResponse {
    Json(json!({"status": "healthy"}))
}

/// Validate the `Authorization` header against the configured auth.
fn validate_auth(
    headers: &HeaderMap,
    auth: &AuthConfig,
) -> Result<(), (StatusCode, Json<JsonValue>)> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Unauthorized"})),
            )
        })?;

    match auth {
        AuthConfig::Basic { username, password } => {
            let encoded = auth_header.strip_prefix("Basic ").ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Unauthorized"})),
                )
            })?;

            let decoded = BASE64_STANDARD.decode(encoded).map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Unauthorized"})),
                )
            })?;

            let decoded_str = String::from_utf8(decoded).map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Unauthorized"})),
                )
            })?;

            let expected = format!("{}:{}", username, password);
            if decoded_str != expected {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Unauthorized"})),
                ));
            }
        }
        AuthConfig::Bearer { token } => {
            let provided = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Unauthorized"})),
                )
            })?;

            if provided != token {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Unauthorized"})),
                ));
            }
        }
    }

    Ok(())
}

/// Ingest handler for requests without path parameters.
async fn ingest_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    process_ingest(state, HashMap::new(), headers, body).await
}

/// Ingest handler for requests with path parameters.
async fn ingest_handler_with_path_params(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    process_ingest(state, params, headers, body).await
}

/// Core ingest logic shared by both handler variants.
async fn process_ingest(
    state: Arc<AppState>,
    path_params: HashMap<String, String>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<JsonValue>) {
    // Check authentication
    if let Some(ref auth) = state.auth {
        if let Err(response) = validate_auth(&headers, auth) {
            return response;
        }
    }

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

    // Build metadata from path parameters
    let base_metadata: HashMap<String, Value> = path_params
        .into_iter()
        .map(|(k, v)| (k, Value::String(v)))
        .collect();

    let message_count = messages.len();
    let mut errors: Vec<String> = Vec::new();

    // Build all messages from the JSON array
    let mut batch: Vec<Message> = Vec::with_capacity(message_count);

    for (idx, msg_json) in messages.into_iter().enumerate() {
        if !msg_json.is_object() {
            warn!(index = idx, "Skipping non-object JSON element");
            errors.push(format!("Element {} is not a JSON object", idx));
            continue;
        }

        let bytes = match serde_json::to_vec(&msg_json) {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("Failed to serialize element {}: {}", idx, e));
                continue;
            }
        };

        batch.push(Message {
            bytes,
            metadata: base_metadata.clone(),
            ..Default::default()
        });
    }

    if batch.is_empty() && !errors.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "All elements failed validation",
                "errors": errors
            })),
        );
    }

    let accepted = batch.len();

    if state.acknowledgment {
        // Create a single callback for the entire batch — the runtime's
        // run_input_batch() wraps it in BeginStream/EndStream and fires
        // the callback when all messages in the stream complete.
        let (callback_tx, callback_rx) = new_callback_chan();

        if let Err(e) = state.sender.send_async((batch, Some(callback_tx))).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": format!("Failed to send batch: {}", e)
                })),
            );
        }

        // Await the single stream-level callback
        match callback_rx.await {
            Ok(Status::Processed) => {}
            Ok(Status::Errored(errs)) => {
                errors.extend(errs);
            }
            Err(_) => {
                errors.push("Callback channel closed".to_string());
            }
        }
    } else {
        // Fire and forget — no callback
        if let Err(e) = state.sender.send_async((batch, None)).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": format!("Failed to send batch: {}", e)
                })),
            );
        }
    }

    if errors.is_empty() {
        (
            StatusCode::OK,
            Json(json!({
                "accepted": accepted,
                "processed": accepted
            })),
        )
    } else if accepted == 0 {
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
                "accepted": accepted,
                "processed": accepted - errors.len(),
                "errors": errors
            })),
        )
    }
}

/// Start an HTTPS server using TLS over manually-accepted TCP connections.
#[cfg(feature = "tls")]
async fn start_tls_server(
    listener: tokio::net::TcpListener,
    app: Router<()>,
    tls_config: &ServerTlsConfig,
    shutdown_rx: oneshot::Receiver<()>,
    addr: &SocketAddr,
    path: &str,
) {
    use tokio_rustls::TlsAcceptor;

    let server_config = match crate::modules::tls::build_server_config(tls_config) {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "Failed to build TLS config for HTTP server");
            return;
        }
    };
    let tls_acceptor = TlsAcceptor::from(std::sync::Arc::new(server_config));

    info!(address = %addr, path = %path, "Starting HTTPS server (TLS)");

    let mut shutdown_rx = shutdown_rx;
    loop {
        tokio::select! {
            result = listener.accept() => {
                let (tcp_stream, peer_addr) = match result {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(error = %e, "HTTPS accept error");
                        continue;
                    }
                };

                let acceptor = tls_acceptor.clone();
                let app = app.clone();

                tokio::spawn(async move {
                    let tls_stream = match acceptor.accept(tcp_stream).await {
                        Ok(s) => s,
                        Err(e) => {
                            debug!(error = %e, source = %peer_addr, "TLS handshake failed");
                            return;
                        }
                    };

                    let io = hyper_util::rt::TokioIo::new(tls_stream);
                    let hyper_service = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                        let mut svc = app.clone();
                        let req = req.map(axum::body::Body::new);
                        async move {
                            use tower::Service;
                            svc.call(req).await
                        }
                    });

                    if let Err(e) = hyper_util::server::conn::auto::Builder::new(
                        hyper_util::rt::TokioExecutor::new(),
                    )
                    .serve_connection(io, hyper_service)
                    .await
                    {
                        debug!(error = %e, source = %peer_addr, "HTTPS connection ended");
                    }
                });
            }
            _ = &mut shutdown_rx => {
                info!("HTTPS server shutting down");
                break;
            }
        }
    }
}

/// Stub for when the `tls` feature is not enabled.
#[cfg(not(feature = "tls"))]
async fn start_tls_server(
    _listener: tokio::net::TcpListener,
    _app: Router<()>,
    _tls_config: &ServerTlsConfig,
    _shutdown_rx: oneshot::Receiver<()>,
    _addr: &SocketAddr,
    _path: &str,
) {
    error!("TLS support requires the 'tls' feature to be enabled");
}

/// HTTP Server input.
///
/// Starts an HTTP server that accepts JSON objects via POST requests
/// and feeds them into the processing pipeline.
pub struct HttpServerInput {
    receiver: Receiver<(Vec<Message>, Option<CallbackChan>)>,
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
            auth: config.auth.clone(),
        });

        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let path = config.path.clone();
        let address = config.address.clone();
        let port = config.port;
        let cors_enabled = config.cors_enabled;
        let tls_config = config.tls.clone();

        // Build router — use the path-param-aware handler when the path contains `:` segments
        let has_path_params = path.contains(':');
        let router = if has_path_params {
            Router::new().route(&path, post(ingest_handler_with_path_params))
        } else {
            Router::new().route(&path, post(ingest_handler))
        };
        let mut app = router
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

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!(error = %e, "Failed to bind HTTP server");
                    return;
                }
            };

            if let Some(ref tls) = tls_config {
                start_tls_server(listener, app, tls, shutdown_rx, &addr, &path).await;
            } else {
                info!(address = %addr, path = %path, "Starting HTTP server");
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = shutdown_rx.await;
                        info!("HTTP server shutting down");
                    })
                    .await
                    .unwrap_or_else(|e| error!(error = %e, "HTTP server error"));
            }
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
impl InputBatch for HttpServerInput {
    async fn read_batch(&mut self) -> Result<(MessageBatch, Option<CallbackChan>), Error> {
        match self.receiver.recv_async().await {
            Ok((batch, callback)) => Ok((batch, callback)),
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

    // Validate TLS client_auth if provided
    if let Some(ref tls) = config.tls {
        match tls.client_auth.as_str() {
            "none" | "optional" | "required" => {}
            other => {
                return Err(Error::ConfigFailedValidation(format!(
                    "invalid tls.client_auth '{}': must be 'none', 'optional', or 'required'",
                    other
                )));
            }
        }
    }

    Ok(ExecutionType::InputBatch(Box::new(HttpServerInput::new(
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
    description: "Endpoint path for ingestion. Supports path parameters using :name syntax (e.g. /ingest/:logtype) which are added to message metadata"
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
  auth:
    type: object
    properties:
      type:
        type: string
        enum: ["basic", "bearer"]
        description: "Authentication type"
      username:
        type: string
        description: "Username for basic auth"
      password:
        type: string
        description: "Password for basic auth"
      token:
        type: string
        description: "Token for bearer auth"
    required:
      - type
    description: "Authentication for incoming requests"
  tls:
    type: object
    properties:
      cert:
        type: string
        description: "Server certificate — file path or inline PEM"
      key:
        type: string
        description: "Server private key — file path or inline PEM"
      ca:
        type: string
        description: "CA certificate for client verification — file path or inline PEM"
      client_auth:
        type: string
        default: "none"
        enum: ["none", "optional", "required"]
        description: "Client authentication mode"
    required:
      - cert
      - key
    description: "TLS configuration for HTTPS support"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "http_server".into(),
        ItemType::InputBatch,
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
    fn test_config_with_tls() {
        let yaml = r#"
port: 8443
tls:
  cert: /etc/ssl/server.crt
  key: /etc/ssl/server.key
"#;
        let config: HttpServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.port, 8443);
        let tls = config.tls.as_ref().unwrap();
        assert_eq!(tls.cert, "/etc/ssl/server.crt");
        assert_eq!(tls.key, "/etc/ssl/server.key");
        assert!(tls.ca.is_none());
        assert_eq!(tls.client_auth, "none");
    }

    #[test]
    fn test_config_with_tls_inline_pem() {
        let yaml = r#"
port: 8443
tls:
  cert: |
    -----BEGIN CERTIFICATE-----
    MIIBxTCCAW...
    -----END CERTIFICATE-----
  key: |
    -----BEGIN PRIVATE KEY-----
    MIIEvQ...
    -----END PRIVATE KEY-----
  client_auth: required
  ca: /etc/ssl/ca.crt
"#;
        let config: HttpServerConfig = serde_yaml::from_str(yaml).unwrap();
        let tls = config.tls.as_ref().unwrap();
        assert!(tls.cert.contains("-----BEGIN CERTIFICATE-----"));
        assert!(tls.key.contains("-----BEGIN PRIVATE KEY-----"));
        assert_eq!(tls.client_auth, "required");
        assert_eq!(tls.ca.as_deref(), Some("/etc/ssl/ca.crt"));
    }

    #[test]
    fn test_config_deserialization_basic_auth() {
        let yaml = r#"
port: 8080
auth:
  type: basic
  username: admin
  password: secret123
"#;
        let config: HttpServerConfig = serde_yaml::from_str(yaml).expect("deserialize basic auth");
        let auth = config.auth.expect("auth should be present");
        match auth {
            AuthConfig::Basic { username, password } => {
                assert_eq!(username, "admin");
                assert_eq!(password, "secret123");
            }
            _ => panic!("Expected Basic auth"),
        }
    }

    #[test]
    fn test_config_deserialization_bearer_auth() {
        let yaml = r#"
port: 8080
auth:
  type: bearer
  token: my-api-token
"#;
        let config: HttpServerConfig = serde_yaml::from_str(yaml).expect("deserialize bearer auth");
        let auth = config.auth.expect("auth should be present");
        match auth {
            AuthConfig::Bearer { token } => {
                assert_eq!(token, "my-api-token");
            }
            _ => panic!("Expected Bearer auth"),
        }
    }

    #[test]
    fn test_config_no_auth() {
        let yaml = r#"
port: 8080
"#;
        let config: HttpServerConfig = serde_yaml::from_str(yaml).expect("deserialize no auth");
        assert!(config.auth.is_none());
    }

    #[test]
    fn test_config_with_path_params() {
        let yaml = r#"
port: 8080
path: "/ingest/:logtype"
"#;
        let config: HttpServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.path, "/ingest/:logtype");
    }

    #[test]
    fn test_config_with_multiple_path_params() {
        let yaml = r#"
port: 8080
path: "/ingest/:source/:logtype"
"#;
        let config: HttpServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.path, "/ingest/:source/:logtype");
    }

    #[test]
    fn test_register_http_server() {
        let result = register_http_server();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
