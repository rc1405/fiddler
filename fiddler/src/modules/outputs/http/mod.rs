//! HTTP client output module for sending data to HTTP endpoints.
//!
//! This module provides output implementations for sending messages via HTTP
//! POST or PUT requests, with support for authentication and batching.
//!
//! # Configuration
//!
//! ```yaml
//! output:
//!   http:
//!     url: "https://api.example.com/events"  # Required: target endpoint
//!     method: "POST"                          # Optional: POST or PUT (default: POST)
//!     headers:                                # Optional: custom headers
//!       Content-Type: "application/json"
//!     auth:                                   # Optional: authentication
//!       type: "bearer"
//!       token: "secret-token"
//!     timeout_secs: 30                        # Optional: request timeout (default: 30)
//!     batch:                                  # Optional: enables batch mode
//!       size: 100
//!       duration: "5s"
//!       format: "json_array"                  # json_array or ndjson (default: ndjson)
//! ```

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::modules::tls::ClientTlsConfig;
use crate::{BatchingPolicy, Closer, Error, Message, MessageBatch, Output, OutputBatch};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use reqwest::{Client, Method};
use serde::Deserialize;
use serde_yaml::Value;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use tracing::{debug, error};

const DEFAULT_METHOD: &str = "POST";
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_BATCH_FORMAT: &str = "json_array";

/// HTTP output configuration.
#[derive(Deserialize, Clone)]
pub struct HttpOutputConfig {
    /// Target HTTP endpoint URL (required).
    pub url: String,
    /// HTTP method: POST or PUT (default: POST).
    #[serde(default = "default_method")]
    pub method: String,
    /// Custom HTTP headers.
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Authentication configuration.
    pub auth: Option<AuthConfig>,
    /// Request timeout in seconds (default: 30).
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Batching policy (enables batch mode if present).
    #[serde(default)]
    pub batch: Option<BatchConfig>,
    /// TLS configuration for custom CA certificates and client certificates.
    pub tls: Option<ClientTlsConfig>,
}

/// Batch configuration with format option.
#[derive(Deserialize, Clone)]
pub struct BatchConfig {
    /// Common batching policy (size, duration, max_batch_bytes).
    #[serde(flatten)]
    pub policy: BatchingPolicy,
    /// Batch format: "json_array" or "ndjson" (default: ndjson).
    #[serde(default = "default_batch_format")]
    pub format: String,
}

/// Authentication configuration.
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
        /// Token value.
        token: String,
    },
}

fn default_method() -> String {
    DEFAULT_METHOD.to_string()
}

fn default_timeout_secs() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

fn default_batch_format() -> String {
    DEFAULT_BATCH_FORMAT.to_string()
}

/// Build a configured reqwest client.
fn build_client(timeout_secs: u64, tls_config: Option<&ClientTlsConfig>) -> Result<Client, Error> {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .pool_max_idle_per_host(2)
        .pool_idle_timeout(Duration::from_secs(90));

    if let Some(tls) = tls_config {
        builder = crate::modules::tls::configure_reqwest_tls(builder, tls)?;
    }

    builder
        .build()
        .map_err(|e| Error::ExecutionError(format!("Failed to build HTTP client: {}", e)))
}

/// Build a request with headers and auth.
fn build_request(
    client: &Client,
    method: &Method,
    url: &str,
    headers: &HashMap<String, String>,
    auth: &Option<AuthConfig>,
    body: Vec<u8>,
) -> reqwest::RequestBuilder {
    let mut req = client.request(method.clone(), url).body(body);

    for (key, value) in headers {
        req = req.header(key.as_str(), value.as_str());
    }

    match auth {
        Some(AuthConfig::Basic { username, password }) => {
            req = req.basic_auth(username, Some(password));
        }
        Some(AuthConfig::Bearer { token }) => {
            req = req.bearer_auth(token);
        }
        None => {}
    }

    req
}

/// Send a request and handle the response.
async fn send_request(req: reqwest::RequestBuilder) -> Result<(), Error> {
    let response = req
        .send()
        .await
        .map_err(|e| Error::OutputError(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_client_error() {
            return Err(Error::UnRetryable(format!(
                "HTTP request failed with status {}: {}",
                status, body
            )));
        }
        error!("HTTP request failed with status {}: {}", status, body);
        return Err(Error::OutputError(format!(
            "HTTP request failed with status {}: {}",
            status, body
        )));
    }

    Ok(())
}

/// Simple HTTP output for single messages.
pub struct HttpOutput {
    client: Client,
    url: String,
    method: Method,
    headers: HashMap<String, String>,
    auth: Option<AuthConfig>,
}

impl HttpOutput {
    /// Creates a new HTTP output.
    pub fn new(config: HttpOutputConfig) -> Result<Self, Error> {
        let method = Method::from_str(&config.method.to_uppercase())
            .map_err(|_| Error::ConfigFailedValidation("Invalid HTTP method".into()))?;

        // Validate URL
        reqwest::Url::parse(&config.url)
            .map_err(|e| Error::ConfigFailedValidation(format!("Invalid URL: {}", e)))?;

        let client = build_client(config.timeout_secs, config.tls.as_ref())?;

        debug!(url = %config.url, method = %config.method, "HTTP output initialized");

        Ok(Self {
            client,
            url: config.url,
            method,
            headers: config.headers,
            auth: config.auth,
        })
    }
}

#[async_trait]
impl Output for HttpOutput {
    async fn write(&mut self, message: Message) -> Result<(), Error> {
        let req = build_request(
            &self.client,
            &self.method,
            &self.url,
            &self.headers,
            &self.auth,
            message.bytes,
        );
        send_request(req).await
    }
}

#[async_trait]
impl Closer for HttpOutput {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("HTTP output closing");
        Ok(())
    }
}

/// Batch HTTP output for sending multiple messages per request.
pub struct HttpBatchOutput {
    client: Client,
    url: String,
    method: Method,
    headers: HashMap<String, String>,
    auth: Option<AuthConfig>,
    batch_size: usize,
    interval: Duration,
    use_json_array: bool,
    max_batch_bytes: usize,
}

impl HttpBatchOutput {
    /// Creates a new batch HTTP output.
    pub fn new(config: HttpOutputConfig) -> Result<Self, Error> {
        let method = Method::from_str(&config.method.to_uppercase())
            .map_err(|_| Error::ConfigFailedValidation("Invalid HTTP method".into()))?;

        // Validate URL
        reqwest::Url::parse(&config.url)
            .map_err(|e| Error::ConfigFailedValidation(format!("Invalid URL: {}", e)))?;

        let client = build_client(config.timeout_secs, config.tls.as_ref())?;

        let batch_config = config.batch.as_ref();
        let batch_size = batch_config.map_or(500, |b| b.policy.effective_size());
        let interval =
            batch_config.map_or(Duration::from_secs(10), |b| b.policy.effective_duration());
        let max_batch_bytes =
            batch_config.map_or(10_485_760, |b| b.policy.effective_max_batch_bytes());

        let use_json_array = batch_config
            .map(|b| b.format == "json_array")
            .unwrap_or(false);

        debug!(
            url = %config.url,
            method = %config.method,
            batch_size = batch_size,
            use_json_array = use_json_array,
            "HTTP batch output initialized"
        );

        Ok(Self {
            client,
            url: config.url,
            method,
            headers: config.headers,
            auth: config.auth,
            batch_size,
            interval,
            use_json_array,
            max_batch_bytes,
        })
    }

    /// Format messages as JSON array.
    fn format_json_array(&self, messages: &MessageBatch) -> Result<Vec<u8>, Error> {
        let mut json_values: Vec<serde_json::Value> = Vec::with_capacity(messages.len());
        for (i, msg) in messages.iter().enumerate() {
            let value: serde_json::Value = serde_json::from_slice(&msg.bytes).map_err(|e| {
                Error::OutputError(format!(
                    "Failed to parse message {} as JSON for batch: {}",
                    i, e
                ))
            })?;
            json_values.push(value);
        }

        serde_json::to_vec(&json_values)
            .map_err(|e| Error::OutputError(format!("JSON serialization failed: {}", e)))
    }

    /// Format messages as newline-delimited JSON.
    fn format_ndjson(&self, messages: &MessageBatch) -> Vec<u8> {
        let total_bytes: usize = messages.iter().map(|m| m.bytes.len()).sum();
        let mut result = Vec::with_capacity(total_bytes + messages.len());
        for (i, msg) in messages.iter().enumerate() {
            if i > 0 {
                result.push(b'\n');
            }
            result.extend_from_slice(&msg.bytes);
        }
        result
    }
}

#[async_trait]
impl OutputBatch for HttpBatchOutput {
    async fn write_batch(&mut self, messages: MessageBatch) -> Result<(), Error> {
        if messages.is_empty() {
            return Ok(());
        }

        let body = if self.use_json_array {
            self.format_json_array(&messages)?
        } else {
            self.format_ndjson(&messages)
        };

        let content_type = if self.use_json_array {
            "application/json"
        } else {
            "application/x-ndjson"
        };

        let mut req = build_request(
            &self.client,
            &self.method,
            &self.url,
            &self.headers,
            &self.auth,
            body,
        );

        // Set Content-Type if not already provided in custom headers
        if !self.headers.contains_key("Content-Type") && !self.headers.contains_key("content-type")
        {
            req = req.header("Content-Type", content_type);
        }

        send_request(req).await?;

        debug!(count = messages.len(), "Sent batch to HTTP endpoint");
        Ok(())
    }

    async fn batch_size(&self) -> usize {
        self.batch_size
    }

    async fn interval(&self) -> Duration {
        self.interval
    }

    async fn max_batch_bytes(&self) -> usize {
        self.max_batch_bytes
    }
}

#[async_trait]
impl Closer for HttpBatchOutput {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("HTTP batch output closing");
        Ok(())
    }
}

/// Validate shared HTTP output configuration.
fn validate_http_config(config: &HttpOutputConfig) -> Result<(), Error> {
    if config.url.is_empty() {
        return Err(Error::ConfigFailedValidation("url is required".into()));
    }

    let method_upper = config.method.to_uppercase();
    if !["POST", "PUT"].contains(&method_upper.as_str()) {
        return Err(Error::ConfigFailedValidation(
            "method must be POST or PUT".into(),
        ));
    }

    if let Some(ref batch) = config.batch {
        if !["json_array", "ndjson"].contains(&batch.format.as_str()) {
            return Err(Error::ConfigFailedValidation(
                "batch.format must be 'json_array' or 'ndjson'".into(),
            ));
        }
    }

    Ok(())
}

#[fiddler_registration_func]
fn create_http_output(conf: Value) -> Result<ExecutionType, Error> {
    let config: HttpOutputConfig = serde_yaml::from_value(conf)?;
    validate_http_config(&config)?;

    // Choose Output vs OutputBatch based on batch config
    if config.batch.is_some() {
        Ok(ExecutionType::OutputBatch(Box::new(HttpBatchOutput::new(
            config,
        )?)))
    } else {
        Ok(ExecutionType::Output(Box::new(HttpOutput::new(config)?)))
    }
}

#[fiddler_registration_func]
fn create_http_batch_output(conf: Value) -> Result<ExecutionType, Error> {
    let mut config: HttpOutputConfig = serde_yaml::from_value(conf)?;
    validate_http_config(&config)?;

    // http_batch always runs in batch mode; apply defaults when batch block is absent
    if config.batch.is_none() {
        config.batch = Some(BatchConfig {
            policy: BatchingPolicy::default(),
            format: default_batch_format(),
        });
    }

    Ok(ExecutionType::OutputBatch(Box::new(HttpBatchOutput::new(
        config,
    )?)))
}

/// Registers the HTTP output plugin.
pub(crate) fn register_http() -> Result<(), Error> {
    let config = r#"type: object
required:
  - url
properties:
  url:
    type: string
    description: "Target HTTP endpoint URL"
  method:
    type: string
    enum: ["POST", "PUT"]
    default: "POST"
    description: "HTTP method"
  headers:
    type: object
    additionalProperties:
      type: string
    description: "Custom HTTP headers"
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
    description: "Authentication (type: basic with username/password, or bearer with token)"
  timeout_secs:
    type: integer
    default: 30
    description: "Request timeout in seconds"
  batch:
    type: object
    properties:
      size:
        type: integer
        description: "Batch size (default: 500)"
      duration:
        type: string
        description: "Flush interval (default: 10s)"
      format:
        type: string
        enum: ["json_array", "ndjson"]
        default: "ndjson"
        description: "Batch format"
      max_batch_bytes:
        type: integer
        default: 10485760
        description: "Maximum cumulative byte size per batch (default: 10MB)"
    description: "Batching configuration (enables batch mode)"
  tls:
    type: object
    properties:
      ca:
        type: string
        description: "CA certificate — file path or inline PEM"
      cert:
        type: string
        description: "Client certificate for mTLS — file path or inline PEM"
      key:
        type: string
        description: "Client private key for mTLS — file path or inline PEM"
      skip_verify:
        type: boolean
        default: false
        description: "Skip server certificate verification"
    description: "TLS configuration for custom certificates"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    // Register for both Output (single message) and OutputBatch (batch mode)
    // The create_http_output function returns the appropriate type based on batch config
    register_plugin(
        "http".into(),
        ItemType::Output,
        conf_spec.clone(),
        create_http_output,
    )?;
    register_plugin(
        "http_batch".into(),
        ItemType::OutputBatch,
        conf_spec,
        create_http_batch_output,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization_simple() {
        let yaml = r#"
url: "https://api.example.com/events"
method: "POST"
headers:
  Content-Type: "application/json"
timeout_secs: 60
"#;
        let config: HttpOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url, "https://api.example.com/events");
        assert_eq!(config.method, "POST");
        assert_eq!(
            config.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(config.timeout_secs, 60);
        assert!(config.batch.is_none());
    }

    #[test]
    fn test_config_deserialization_with_batch() {
        let yaml = r#"
url: "https://api.example.com/bulk"
method: "PUT"
batch:
  size: 100
  duration: "5s"
  format: "json_array"
"#;
        let config: HttpOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url, "https://api.example.com/bulk");
        assert_eq!(config.method, "PUT");
        assert!(config.batch.is_some());
        let batch = config.batch.unwrap();
        assert_eq!(batch.policy.size, Some(100));
        let expected = parse_duration::parse("5s").unwrap();
        assert_eq!(batch.policy.duration, Some(expected));
        assert_eq!(batch.format, "json_array");
    }

    #[test]
    fn test_config_deserialization_basic_auth() {
        let yaml = r#"
url: "https://api.example.com/events"
auth:
  type: "basic"
  username: "user"
  password: "pass"
"#;
        let config: HttpOutputConfig = serde_yaml::from_str(yaml).unwrap();
        match config.auth {
            Some(AuthConfig::Basic { username, password }) => {
                assert_eq!(username, "user");
                assert_eq!(password, "pass");
            }
            _ => panic!("Expected basic auth"),
        }
    }

    #[test]
    fn test_config_deserialization_bearer_auth() {
        let yaml = r#"
url: "https://api.example.com/events"
auth:
  type: "bearer"
  token: "secret-token"
"#;
        let config: HttpOutputConfig = serde_yaml::from_str(yaml).unwrap();
        match config.auth {
            Some(AuthConfig::Bearer { token }) => {
                assert_eq!(token, "secret-token");
            }
            _ => panic!("Expected bearer auth"),
        }
    }

    #[test]
    fn test_config_defaults() {
        let yaml = r#"url: "https://api.example.com""#;
        let config: HttpOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.method, "POST");
        assert_eq!(config.timeout_secs, 30);
        assert!(config.headers.is_empty());
        assert!(config.auth.is_none());
        assert!(config.batch.is_none());
    }

    #[test]
    fn test_config_deserialization_with_tls() {
        let yaml = r#"
url: "https://api.example.com/events"
tls:
  ca: |
    -----BEGIN CERTIFICATE-----
    MIIBxTCCAW...
    -----END CERTIFICATE-----
  cert: /etc/ssl/client.crt
  key: /etc/ssl/client.key
  skip_verify: false
"#;
        let config: HttpOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url, "https://api.example.com/events");
        let tls = config.tls.as_ref().unwrap();
        assert!(tls.ca.as_deref().unwrap().contains("-----BEGIN"));
        assert_eq!(tls.cert.as_deref(), Some("/etc/ssl/client.crt"));
        assert_eq!(tls.key.as_deref(), Some("/etc/ssl/client.key"));
        assert!(!tls.skip_verify);
    }

    #[test]
    fn test_config_deserialization_tls_skip_verify() {
        let yaml = r#"
url: "https://api.example.com/events"
tls:
  skip_verify: true
"#;
        let config: HttpOutputConfig = serde_yaml::from_str(yaml).unwrap();
        let tls = config.tls.as_ref().unwrap();
        assert!(tls.skip_verify);
        assert!(tls.ca.is_none());
        assert!(tls.cert.is_none());
    }

    #[test]
    fn test_register_http() {
        let result = register_http();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
