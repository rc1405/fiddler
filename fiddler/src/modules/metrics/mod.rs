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
use crate::Error;
use async_trait::async_trait;

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

/// Trait for metrics backends.
///
/// Implementations of this trait are responsible for recording and exposing
/// metrics from the fiddler runtime. The trait is designed to be lightweight
/// and non-blocking to avoid impacting pipeline performance.
#[async_trait]
pub trait Metrics: Send + Sync {
    /// Records current metrics values to the backend.
    ///
    /// This method is called periodically by the runtime to update metrics.
    /// Implementations should be fast and non-blocking.
    ///
    /// # Arguments
    ///
    /// * `total_received` - Total messages received from input
    /// * `total_completed` - Messages successfully processed through all outputs
    /// * `total_process_errors` - Messages that encountered processing errors
    /// * `total_output_errors` - Messages that encountered output errors
    /// * `streams_started` - Number of streams started
    /// * `streams_completed` - Number of streams completed
    /// * `duplicates_rejected` - Duplicate messages rejected
    /// * `stale_entries_removed` - Stale entries cleaned up
    /// * `in_flight` - Current number of messages being processed
    /// * `throughput_per_sec` - Current throughput in messages per second
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
    );

    /// Closes the metrics backend and flushes any pending data.
    async fn close(&self) -> Result<(), Error> {
        Ok(())
    }
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
    fn record(
        &self,
        _total_received: u64,
        _total_completed: u64,
        _total_process_errors: u64,
        _total_output_errors: u64,
        _streams_started: u64,
        _streams_completed: u64,
        _duplicates_rejected: u64,
        _stale_entries_removed: u64,
        _in_flight: usize,
        _throughput_per_sec: f64,
    ) {
        // No-op: metrics are disabled
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
        let metrics = NoOpMetrics::new();
        // Should not panic
        metrics.record(100, 90, 5, 5, 10, 8, 2, 1, 50, 123.45);
    }

    #[tokio::test]
    async fn test_noop_metrics_close() {
        let metrics = NoOpMetrics::new();
        assert!(metrics.close().await.is_ok());
    }

    #[tokio::test]
    async fn test_create_metrics_without_config() {
        let metrics = create_metrics(None).await.unwrap();
        // Should create a no-op metrics instance
        metrics.record(0, 0, 0, 0, 0, 0, 0, 0, 0, 0.0);
    }

    #[tokio::test]
    async fn test_create_metrics_with_empty_config() {
        let config = crate::config::MetricsConfig::default();
        let metrics = create_metrics(Some(&config)).await.unwrap();
        // Should create a no-op metrics instance when no backend specified
        metrics.record(0, 0, 0, 0, 0, 0, 0, 0, 0, 0.0);
    }

    #[cfg(feature = "prometheus")]
    #[tokio::test]
    async fn test_create_metrics_with_prometheus_config() {
        // Register the prometheus plugin first
        prometheus::register_prometheus().unwrap();

        use std::collections::HashMap;
        let mut extra = HashMap::new();
        extra.insert(
            "prometheus".to_string(),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
        let config = crate::config::MetricsConfig { label: None, extra };
        let metrics = create_metrics(Some(&config)).await.unwrap();
        // Should create a prometheus metrics instance
        metrics.record(100, 90, 5, 5, 10, 8, 2, 1, 50, 123.45);
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
