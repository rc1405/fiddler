//! Metrics module for observability and monitoring.
//!
//! This module provides a trait-based abstraction for recording metrics
//! from the fiddler runtime. Different backends can be implemented by
//! adding new submodules that implement the `Metrics` trait.
//!
//! # Configuration
//!
//! Metrics can be configured in the pipeline YAML:
//!
//! ```yaml
//! metrics:
//!   prometheus:
//!     # prometheus-specific configuration
//! ```
//!
//! If no metrics configuration is provided, a no-op implementation is used.

use crate::config::{ExecutionType, ItemType};
use crate::{Closer, Error, MetricEntry, Metrics};
use async_trait::async_trait;
use tracing::debug;

#[cfg(feature = "prometheus")]
pub mod prometheus;

/// Registers all available metrics plugins.
///
/// This function should be called during plugin initialization to register
/// all metrics backends that are enabled via feature flags.
pub(crate) fn register_plugins() -> Result<(), Error> {
    #[cfg(feature = "prometheus")]
    prometheus::register_prometheus()?;

    Ok(())
}

/// No-op metrics implementation used when no metrics backend is configured.
///
/// This implementation does nothing, ensuring zero overhead when metrics
/// are not needed.
#[derive(Debug, Default)]
pub struct NoOpMetrics;

impl NoOpMetrics {
    /// Creates a new no-op metrics instance.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Metrics for NoOpMetrics {
    fn record(&self, _metric: MetricEntry) {
        // No-op: metrics are disabled
    }
}

#[async_trait]
impl Closer for NoOpMetrics {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("noop metrics backend closing");
        Ok(())
    }
}

/// Creates a metrics backend based on the provided configuration.
///
/// The configuration uses a dynamic key-value pattern similar to inputs/outputs.
/// The first key in the `extra` HashMap determines which backend to create.
/// The backend must be registered via the plugin registration system.
///
/// Returns a `NoOpMetrics` instance if no configuration is provided.
/// Returns an error if the requested backend is not registered.
pub async fn create_metrics(
    config: Option<&crate::config::MetricsConfig>,
) -> Result<Box<dyn Metrics>, Error> {
    match config {
        Some(cfg) => {
            // Check if any metrics backend is configured
            if cfg.extra.is_empty() {
                return Ok(Box::new(NoOpMetrics::new()));
            }

            // Use the standard plugin lookup mechanism
            let parsed_item =
                crate::config::parse_configuration_item(ItemType::Metrics, &cfg.extra).await?;

            // Call the creator to get the metrics backend
            let execution_type = (parsed_item.creator)(parsed_item.config).await?;

            // Extract the Metrics from ExecutionType
            match execution_type {
                ExecutionType::Metrics(m) => Ok(m),
                _ => Err(Error::Validation(
                    "Metrics plugin returned invalid execution type".into(),
                )),
            }
        }
        None => Ok(Box::new(NoOpMetrics::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_metrics_record() {
        let mut metrics = NoOpMetrics::new();
        // Should not panic
        metrics.record(MetricEntry {
            total_received: 100,
            total_completed: 90,
            total_process_errors: 5,
            total_output_errors: 5,
            streams_started: 10,
            streams_completed: 8,
            duplicates_rejected: 2,
            stale_entries_removed: 1,
            in_flight: 50,
            throughput_per_sec: 123.45,
        });
    }

    #[tokio::test]
    async fn test_noop_metrics_close() {
        let mut metrics = NoOpMetrics::new();
        assert!(metrics.close().await.is_ok());
    }

    #[tokio::test]
    async fn test_create_metrics_without_config() {
        let mut metrics = create_metrics(None).await.unwrap();
        // Should create a no-op metrics instance
        metrics.record(MetricEntry {
            total_received: 0,
            total_completed: 0,
            total_process_errors: 0,
            total_output_errors: 0,
            streams_started: 0,
            streams_completed: 0,
            duplicates_rejected: 0,
            stale_entries_removed: 0,
            in_flight: 0,
            throughput_per_sec: 0.0,
        });
    }

    #[tokio::test]
    async fn test_create_metrics_with_empty_config() {
        let config = crate::config::MetricsConfig::default();
        let mut metrics = create_metrics(Some(&config)).await.unwrap();
        // Should create a no-op metrics instance when no backend specified
        metrics.record(MetricEntry {
            total_received: 0,
            total_completed: 0,
            total_process_errors: 0,
            total_output_errors: 0,
            streams_started: 0,
            streams_completed: 0,
            duplicates_rejected: 0,
            stale_entries_removed: 0,
            in_flight: 0,
            throughput_per_sec: 0.0,
        });
    }

    #[cfg(feature = "prometheus")]
    #[tokio::test]
    async fn test_create_metrics_with_prometheus_config() {
        // Registration should succeed (or already be registered)
        let result = prometheus::register_prometheus();
        // May return DuplicateRegisteredName if already registered by another test
        assert!(result.is_ok() || matches!(result, Err(crate::Error::DuplicateRegisteredName(_))));

        use std::collections::HashMap;
        let mut extra = HashMap::new();
        extra.insert(
            "prometheus".to_string(),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
        let config = crate::config::MetricsConfig { label: None, extra };
        let mut metrics = create_metrics(Some(&config)).await.unwrap();
        // Should create a prometheus metrics instance
        metrics.record(MetricEntry {
            total_received: 100,
            total_completed: 90,
            total_process_errors: 5,
            total_output_errors: 5,
            streams_started: 10,
            streams_completed: 8,
            duplicates_rejected: 2,
            stale_entries_removed: 1,
            in_flight: 50,
            throughput_per_sec: 123.45,
        });
    }

    #[tokio::test]
    async fn test_create_metrics_with_unknown_backend() {
        use std::collections::HashMap;
        let mut extra = HashMap::new();
        extra.insert(
            "unknown_backend".to_string(),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
        let config = crate::config::MetricsConfig { label: None, extra };
        // Should return an error for unknown backend
        let result = create_metrics(Some(&config)).await;
        assert!(result.is_err());
        match result {
            Err(crate::Error::ConfigurationItemNotFound(name)) => {
                assert_eq!(name, "unknown_backend");
            }
            _ => panic!("Expected ConfigurationItemNotFound error"),
        }
    }
}
