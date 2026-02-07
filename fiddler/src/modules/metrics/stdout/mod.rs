//! Stdout JSON metrics backend.
//!
//! This module provides a metrics implementation that outputs counters
//! to stdout as JSON strings.
//!
//! # Configuration
//!
//! ```yaml
//! metrics:
//!   stdout:
//!     pretty: false  # Optional: use pretty-printed JSON (default: false)
//! ```

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Error;
use crate::{Closer, MetricEntry, Metrics};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use serde::Deserialize;
use serde_yaml::Value;
use tracing::debug;

/// Stdout-specific configuration options.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct StdoutConfig {
    /// Whether to pretty-print the JSON output.
    #[serde(default)]
    pub pretty: bool,
}

/// Stdout JSON metrics backend.
///
/// Records metrics by outputting them to stdout as JSON strings.
/// Each call to `record` outputs a single JSON line.
#[derive(Debug)]
pub struct StdoutMetrics {
    config: StdoutConfig,
}

impl StdoutMetrics {
    /// Creates a new Stdout metrics instance from configuration.
    pub fn new(config: Value) -> Result<Self, Error> {
        let stdout_config: StdoutConfig = serde_yaml::from_value(config)?;
        debug!(
            "Stdout metrics backend initialized with pretty={}",
            stdout_config.pretty
        );
        Ok(Self {
            config: stdout_config,
        })
    }
}

impl Default for StdoutMetrics {
    fn default() -> Self {
        Self {
            config: StdoutConfig::default(),
        }
    }
}

#[async_trait]
impl Metrics for StdoutMetrics {
    fn record(&mut self, metric: MetricEntry) {
        let json_output = if self.config.pretty {
            serde_json::to_string_pretty(&metric)
        } else {
            serde_json::to_string(&metric)
        };

        match json_output {
            Ok(json) => println!("{}", json),
            Err(e) => {
                tracing::error!(error = %e, "Failed to serialize metrics to JSON");
            }
        }
    }
}

#[async_trait]
impl Closer for StdoutMetrics {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("Stdout metrics backend closing");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_stdout(conf: Value) -> Result<ExecutionType, Error> {
    Ok(ExecutionType::Metrics(Box::new(StdoutMetrics::new(conf)?)))
}

/// Registers the stdout metrics plugin.
pub(crate) fn register_stdout() -> Result<(), Error> {
    let config = r#"type: object
properties:
  pretty:
    type: boolean
    default: false"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("stdout".into(), ItemType::Metrics, conf_spec, create_stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdout_metrics_default() {
        let metrics = StdoutMetrics::default();
        assert!(!metrics.config.pretty);
    }

    #[test]
    fn test_stdout_metrics_with_pretty() {
        let config = serde_yaml::from_str("pretty: true").unwrap();
        let metrics = StdoutMetrics::new(config).unwrap();
        assert!(metrics.config.pretty);
    }

    #[test]
    fn test_stdout_metrics_record() {
        // This test outputs to stdout, which is captured by cargo test
        let mut metrics = StdoutMetrics::default();
        metrics.record(MetricEntry {
            total_received: 100,
            total_completed: 90,
            total_process_errors: 5,
            total_output_errors: 5,
            total_filtered: 0,
            streams_started: 10,
            streams_completed: 8,
            duplicates_rejected: 2,
            stale_entries_removed: 1,
            in_flight: 50,
            throughput_per_sec: 123.45,
            cpu_usage_percent: None,
            memory_used_bytes: None,
            memory_total_bytes: None,
            input_bytes: 1000,
            output_bytes: 900,
            bytes_per_sec: 90.0,
            latency_avg_ms: 5.5,
            latency_min_ms: 1.0,
            latency_max_ms: 15.0,
        });
    }

    #[tokio::test]
    async fn test_stdout_metrics_close() {
        let mut metrics = StdoutMetrics::default();
        assert!(metrics.close().await.is_ok());
    }

    #[test]
    fn test_register_stdout() {
        let result = register_stdout();
        // May return DuplicateRegisteredName if already registered by another test
        assert!(result.is_ok() || matches!(result, Err(crate::Error::DuplicateRegisteredName(_))));
    }
}
