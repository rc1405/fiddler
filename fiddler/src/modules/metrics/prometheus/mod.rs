//! Prometheus metrics backend.
//!
//! This module provides a Prometheus-compatible metrics implementation
//! that exposes metrics via the metrics-exporter-prometheus crate.
//!
//! # Configuration
//!
//! ```yaml
//! metrics:
//!   prometheus: {}
//! ```

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Error;
use crate::{Closer, Metrics};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use metrics::{counter, gauge};
use metrics_exporter_prometheus::PrometheusBuilder;
use serde::Deserialize;
use serde_yaml::Value;
use std::sync::Once;
use tracing::{debug, warn};

static PROMETHEUS_INIT: Once = Once::new();

/// Prometheus-specific configuration options.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct PrometheusConfig {
    // Future configuration options can be added here
    // For example: port, endpoint path, labels, etc.
}

/// Prometheus metrics backend.
///
/// Records metrics using the `metrics` crate which are then exposed
/// via a Prometheus-compatible endpoint.
#[derive(Debug)]
pub struct PrometheusMetrics {
    _initialized: bool,
    _config: PrometheusConfig,
}

impl PrometheusMetrics {
    /// Creates a new Prometheus metrics instance from configuration.
    ///
    /// Initializes the Prometheus exporter on first call. Subsequent calls
    /// will reuse the existing exporter.
    pub fn new(config: Value) -> Result<Self, Error> {
        let prometheus_config: PrometheusConfig = serde_yaml::from_value(config)?;
        let mut initialized = false;

        PROMETHEUS_INIT.call_once(|| {
            match PrometheusBuilder::new().install() {
                Ok(_) => {
                    debug!("Prometheus metrics exporter initialized");
                    initialized = true;
                }
                Err(e) => {
                    warn!(error = %e, "Failed to initialize Prometheus exporter, metrics will be recorded but not exposed");
                }
            }
        });

        Ok(Self {
            _initialized: initialized,
            _config: prometheus_config,
        })
    }
}

impl Default for PrometheusMetrics {
    fn default() -> Self {
        Self::new(Value::Null).unwrap_or(Self {
            _initialized: false,
            _config: PrometheusConfig::default(),
        })
    }
}

#[async_trait]
impl Metrics for PrometheusMetrics {
    fn record(
        &self,
        total_received: u64,
        total_completed: u64,
        total_process_errors: u64,
        total_output_errors: u64,
        streams_started: u64,
        streams_completed: u64,
        duplicates_rejected: u64,
        stale_entries_removed: u64,
        in_flight: usize,
        throughput_per_sec: f64,
    ) {
        counter!("fiddler_messages_received_total").absolute(total_received);
        counter!("fiddler_messages_completed_total").absolute(total_completed);
        counter!("fiddler_messages_process_errors_total").absolute(total_process_errors);
        counter!("fiddler_messages_output_errors_total").absolute(total_output_errors);
        counter!("fiddler_streams_started_total").absolute(streams_started);
        counter!("fiddler_streams_completed_total").absolute(streams_completed);
        counter!("fiddler_duplicates_rejected_total").absolute(duplicates_rejected);
        counter!("fiddler_stale_entries_removed_total").absolute(stale_entries_removed);
        gauge!("fiddler_messages_in_flight").set(in_flight as f64);
        gauge!("fiddler_throughput_per_second").set(throughput_per_sec);
    }
}

#[async_trait]
impl Closer for PrometheusMetrics {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("Prometheus metrics backend closing");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_prometheus(conf: Value) -> Result<ExecutionType, Error> {
    Ok(ExecutionType::Metrics(Box::new(PrometheusMetrics::new(
        conf,
    )?)))
}

/// Registers the prometheus metrics plugin.
pub(crate) fn register_prometheus() -> Result<(), Error> {
    let config = "type: object";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "prometheus".into(),
        ItemType::Metrics,
        conf_spec,
        create_prometheus,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prometheus_metrics_record() {
        // Note: This test may not fully initialize prometheus due to Once guard
        // but should not panic
        let metrics = PrometheusMetrics::default();
        metrics.record(100, 90, 5, 5, 10, 8, 2, 1, 50, 123.45);
    }

    #[tokio::test]
    async fn test_prometheus_metrics_close() {
        let mut metrics = PrometheusMetrics::default();
        assert!(metrics.close().await.is_ok());
    }

    #[test]
    fn test_register_prometheus() {
        // Registration should succeed (or already be registered)
        let result = register_prometheus();
        // May return DuplicateRegisteredName if already registered by another test
        assert!(result.is_ok() || matches!(result, Err(crate::Error::DuplicateRegisteredName(_))));
    }
}
