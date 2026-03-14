//! Shared TLS utilities for server-side and client-side certificate configuration.
//!
//! Provides unified handling of PEM certificates from both file paths and inline content.
//! A value starting with `-----BEGIN` is treated as inline PEM content; otherwise it is
//! treated as a file path.

use crate::Error;
use serde::Deserialize;

/// Server-side TLS configuration (used by syslog, http_server).
#[derive(Deserialize, Clone, Debug)]
pub struct ServerTlsConfig {
    /// Server certificate — file path or inline PEM.
    pub cert: String,
    /// Server private key — file path or inline PEM.
    pub key: String,
    /// CA certificate for client verification — file path or inline PEM.
    pub ca: Option<String>,
    /// Client authentication mode: "none", "optional", or "required" (default: "none").
    #[serde(default = "default_client_auth")]
    pub client_auth: String,
}

/// Client-side TLS configuration (used by http, clickhouse, elasticsearch).
#[derive(Deserialize, Clone, Debug)]
pub struct ClientTlsConfig {
    /// CA certificate for server verification — file path or inline PEM.
    pub ca: Option<String>,
    /// Client certificate for mTLS — file path or inline PEM.
    pub cert: Option<String>,
    /// Client private key for mTLS — file path or inline PEM.
    pub key: Option<String>,
    /// Skip server certificate verification (default: false).
    #[serde(default)]
    pub skip_verify: bool,
}

fn default_client_auth() -> String {
    "none".to_string()
}

/// Returns `true` if the value looks like inline PEM content.
fn is_inline_pem(value: &str) -> bool {
    value.trim_start().starts_with("-----BEGIN")
}

/// Read PEM content from either an inline string or a file path.
///
/// If the value starts with `-----BEGIN`, it is returned as bytes directly.
/// Otherwise it is treated as a file path and the file content is read.
pub fn read_pem(value: &str, field_name: &str) -> Result<Vec<u8>, Error> {
    if is_inline_pem(value) {
        Ok(value.as_bytes().to_vec())
    } else {
        std::fs::read(value).map_err(|e| {
            Error::ConfigFailedValidation(format!(
                "failed to read {} file '{}': {}",
                field_name, value, e
            ))
        })
    }
}

// --- Server-side TLS helpers (require tokio-rustls + rustls-pemfile) ---

/// Load certificates from a PEM value (file path or inline).
#[cfg(feature = "tls")]
pub fn load_certs(
    value: &str,
    field_name: &str,
) -> Result<Vec<tokio_rustls::rustls::pki_types::CertificateDer<'static>>, Error> {
    let pem_bytes = read_pem(value, field_name)?;
    let mut cursor = std::io::Cursor::new(pem_bytes);
    let certs: Vec<_> = rustls_pemfile::certs(&mut cursor)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            Error::ConfigFailedValidation(format!("failed to parse {} certs: {}", field_name, e))
        })?;
    if certs.is_empty() {
        return Err(Error::ConfigFailedValidation(format!(
            "no certificates found in {}",
            field_name
        )));
    }
    Ok(certs)
}

/// Load a private key from a PEM value (file path or inline).
#[cfg(feature = "tls")]
pub fn load_private_key(
    value: &str,
    field_name: &str,
) -> Result<tokio_rustls::rustls::pki_types::PrivateKeyDer<'static>, Error> {
    let pem_bytes = read_pem(value, field_name)?;

    // Warn about file permissions on Unix
    if !is_inline_pem(value) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(value) {
                let mode = meta.permissions().mode() & 0o777;
                if mode > 0o600 {
                    tracing::warn!(
                        path = value,
                        mode = format!("{:o}", mode),
                        "TLS private key file permissions are more open than 0600"
                    );
                }
            }
        }
    }

    let mut cursor = std::io::Cursor::new(pem_bytes);
    rustls_pemfile::private_key(&mut cursor)
        .map_err(|e| {
            Error::ConfigFailedValidation(format!("failed to parse {} key: {}", field_name, e))
        })?
        .ok_or_else(|| {
            Error::ConfigFailedValidation(format!("no private key found in {}", field_name))
        })
}

/// Build a `rustls::ServerConfig` from a `ServerTlsConfig`.
#[cfg(feature = "tls")]
pub fn build_server_config(
    config: &ServerTlsConfig,
) -> Result<tokio_rustls::rustls::ServerConfig, Error> {
    use std::sync::Arc;
    use tokio_rustls::rustls;

    let certs = load_certs(&config.cert, "tls.cert")?;
    let key = load_private_key(&config.key, "tls.key")?;

    let server_config = match config.client_auth.as_str() {
        "required" | "optional" => {
            let ca_value = config.ca.as_deref().ok_or_else(|| {
                Error::ConfigFailedValidation(
                    "tls.ca is required when client_auth is not 'none'".into(),
                )
            })?;
            let ca_certs = load_certs(ca_value, "tls.ca")?;
            let mut root_store = rustls::RootCertStore::empty();
            for cert in ca_certs {
                root_store.add(cert).map_err(|e| {
                    Error::ConfigFailedValidation(format!("failed to add CA cert: {}", e))
                })?;
            }
            let verifier = if config.client_auth == "required" {
                rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
                    .build()
                    .map_err(|e| {
                        Error::ConfigFailedValidation(format!(
                            "failed to build client verifier: {}",
                            e
                        ))
                    })?
            } else {
                rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
                    .allow_unauthenticated()
                    .build()
                    .map_err(|e| {
                        Error::ConfigFailedValidation(format!(
                            "failed to build client verifier: {}",
                            e
                        ))
                    })?
            };
            rustls::ServerConfig::builder()
                .with_client_cert_verifier(verifier)
                .with_single_cert(certs, key)
                .map_err(|e| {
                    Error::ConfigFailedValidation(format!("failed to build TLS config: {}", e))
                })?
        }
        _ => rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| {
                Error::ConfigFailedValidation(format!("failed to build TLS config: {}", e))
            })?,
    };

    Ok(server_config)
}

// --- Client-side TLS helpers (require reqwest with rustls-tls) ---

/// Apply `ClientTlsConfig` to a `reqwest::ClientBuilder`.
#[cfg(any(
    feature = "http_client",
    feature = "clickhouse",
    feature = "elasticsearch"
))]
pub fn configure_reqwest_tls(
    mut builder: reqwest::ClientBuilder,
    config: &ClientTlsConfig,
) -> Result<reqwest::ClientBuilder, Error> {
    if config.skip_verify {
        builder = builder.danger_accept_invalid_certs(true);
    }

    if let Some(ref ca) = config.ca {
        let pem_bytes = read_pem(ca, "tls.ca")?;
        let cert = reqwest::tls::Certificate::from_pem(&pem_bytes).map_err(|e| {
            Error::ConfigFailedValidation(format!("failed to parse tls.ca certificate: {}", e))
        })?;
        builder = builder.add_root_certificate(cert);
    }

    if let Some(ref cert) = config.cert {
        let key_value = config.key.as_deref().ok_or_else(|| {
            Error::ConfigFailedValidation("tls.key is required when tls.cert is provided".into())
        })?;
        let cert_pem = read_pem(cert, "tls.cert")?;
        let key_pem = read_pem(key_value, "tls.key")?;

        // reqwest::Identity::from_pem expects cert+key concatenated
        let mut combined = cert_pem;
        combined.extend_from_slice(&key_pem);
        let identity = reqwest::tls::Identity::from_pem(&combined).map_err(|e| {
            Error::ConfigFailedValidation(format!("failed to build TLS identity: {}", e))
        })?;
        builder = builder.identity(identity);
    }

    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_inline_pem() {
        assert!(is_inline_pem(
            "-----BEGIN CERTIFICATE-----\nMIIB...\n-----END CERTIFICATE-----"
        ));
        assert!(is_inline_pem(
            "  -----BEGIN PRIVATE KEY-----\nMIIE...\n-----END PRIVATE KEY-----"
        ));
        assert!(!is_inline_pem("/etc/ssl/cert.pem"));
        assert!(!is_inline_pem("./certs/server.crt"));
    }

    #[test]
    fn test_read_pem_inline() {
        let pem = "-----BEGIN CERTIFICATE-----\nTESTDATA\n-----END CERTIFICATE-----";
        let result = read_pem(pem, "test").expect("should succeed");
        assert_eq!(result, pem.as_bytes());
    }

    #[test]
    fn test_read_pem_file_not_found() {
        let result = read_pem("/nonexistent/path/cert.pem", "test");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("failed to read"), "error was: {}", err);
    }

    #[test]
    fn test_server_tls_config_deserialize() {
        let yaml = r#"
cert: /etc/ssl/server.crt
key: |
  -----BEGIN PRIVATE KEY-----
  MIIEvQ...
  -----END PRIVATE KEY-----
ca: /etc/ssl/ca.crt
client_auth: required
"#;
        let config: ServerTlsConfig = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(config.cert, "/etc/ssl/server.crt");
        assert!(config.key.contains("-----BEGIN PRIVATE KEY-----"));
        assert_eq!(config.ca.as_deref(), Some("/etc/ssl/ca.crt"));
        assert_eq!(config.client_auth, "required");
    }

    #[test]
    fn test_server_tls_config_defaults() {
        let yaml = r#"
cert: /etc/ssl/server.crt
key: /etc/ssl/server.key
"#;
        let config: ServerTlsConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(config.ca.is_none());
        assert_eq!(config.client_auth, "none");
    }

    #[test]
    fn test_client_tls_config_deserialize() {
        let yaml = r#"
ca: |
  -----BEGIN CERTIFICATE-----
  MIIBxTCCAW...
  -----END CERTIFICATE-----
cert: /etc/ssl/client.crt
key: /etc/ssl/client.key
skip_verify: false
"#;
        let config: ClientTlsConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(config.ca.as_deref().unwrap().contains("-----BEGIN"));
        assert_eq!(config.cert.as_deref(), Some("/etc/ssl/client.crt"));
        assert_eq!(config.key.as_deref(), Some("/etc/ssl/client.key"));
        assert!(!config.skip_verify);
    }

    #[test]
    fn test_client_tls_config_defaults() {
        let yaml = "{}";
        let config: ClientTlsConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(config.ca.is_none());
        assert!(config.cert.is_none());
        assert!(config.key.is_none());
        assert!(!config.skip_verify);
    }

    #[test]
    fn test_client_tls_config_skip_verify_only() {
        let yaml = "skip_verify: true";
        let config: ClientTlsConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(config.skip_verify);
    }
}
