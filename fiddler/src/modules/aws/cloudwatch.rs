//! AWS CloudWatch metrics backend.
//!
//! This module provides a metrics implementation that sends metrics
//! to AWS CloudWatch.
//!
//! # Configuration
//!
//! ```yaml
//! metrics:
//!   cloudwatch:
//!     namespace: "Fiddler"           # Optional: CloudWatch namespace (default: "Fiddler")
//!     region: "us-east-1"            # Optional: AWS region (uses default provider if not set)
//!     credentials:                    # Optional: explicit credentials
//!       access_key_id: "..."
//!       secret_access_key: "..."
//!       session_token: "..."         # Optional
//!     include:                        # Optional: list of metrics to include (all if not set)
//!       - total_received
//!       - throughput_per_sec
//!     exclude:                        # Optional: list of metrics to exclude (none if not set)
//!       - stale_entries_removed
//!     dimensions:                     # Optional: additional dimensions for all metrics
//!       - name: "Environment"
//!         value: "production"
//! ```

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Error;
use crate::{Closer, MetricEntry, Metrics};
use async_trait::async_trait;
use aws_sdk_cloudwatch::types::{Dimension, MetricDatum, StandardUnit};
use aws_sdk_cloudwatch::Client;
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Sender};
use serde::Deserialize;
use serde_yaml::Value;
use std::collections::HashSet;
use tracing::{debug, error};

use super::Credentials;

const DEFAULT_NAMESPACE: &str = "Fiddler";
const CHANNEL_BUFFER_SIZE: usize = 100;

/// All available metric names that can be filtered.
const ALL_METRICS: &[&str] = &[
    "total_received",
    "total_completed",
    "total_process_errors",
    "total_output_errors",
    "streams_started",
    "streams_completed",
    "duplicates_rejected",
    "stale_entries_removed",
    "in_flight",
    "throughput_per_sec",
    "input_bytes",
    "output_bytes",
    "bytes_per_sec",
];

/// CloudWatch dimension configuration.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct DimensionConfig {
    /// Dimension name.
    pub name: String,
    /// Dimension value.
    pub value: String,
}

/// CloudWatch-specific configuration options.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct CloudWatchConfig {
    /// CloudWatch namespace for metrics (default: "Fiddler").
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// AWS region. If not specified, uses the default provider chain.
    pub region: Option<String>,
    /// Explicit AWS credentials. If not specified, uses the default provider chain.
    pub credentials: Option<Credentials>,
    /// List of metric names to include. If empty or not set, all metrics are included.
    #[serde(default)]
    pub include: Vec<String>,
    /// List of metric names to exclude. Applied after include filter.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Additional dimensions to add to all metrics.
    #[serde(default)]
    pub dimensions: Vec<DimensionConfig>,
}

fn default_namespace() -> String {
    DEFAULT_NAMESPACE.to_string()
}

/// AWS CloudWatch metrics backend.
///
/// Records metrics by sending them to AWS CloudWatch asynchronously.
/// Uses a bounded channel with a background task for non-blocking operation.
pub struct CloudWatchMetrics {
    sender: Sender<MetricEntry>,
    include_set: HashSet<String>,
    exclude_set: HashSet<String>,
}

impl CloudWatchMetrics {
    /// Creates a new CloudWatch metrics instance from configuration.
    pub async fn new(config: Value) -> Result<Self, Error> {
        let cw_config: CloudWatchConfig = serde_yaml::from_value(config)?;

        // Build AWS config
        let mut aws_config_builder = aws_config::from_env();

        if let Some(region) = &cw_config.region {
            aws_config_builder = aws_config_builder.region(aws_config::Region::new(region.clone()));
        }

        if let Some(creds) = &cw_config.credentials {
            let credentials = aws_sdk_cloudwatch::config::Credentials::new(
                &creds.access_key_id,
                &creds.secret_access_key,
                creds.session_token.clone(),
                None,
                "fiddler-cloudwatch",
            );
            aws_config_builder = aws_config_builder.credentials_provider(credentials);
        }

        let aws_config = aws_config_builder.load().await;
        let client = Client::new(&aws_config);

        // Build dimension list
        let dimensions: Vec<Dimension> = cw_config
            .dimensions
            .iter()
            .map(|d| Dimension::builder().name(&d.name).value(&d.value).build())
            .collect();

        // Build include/exclude sets
        let include_set: HashSet<String> = if cw_config.include.is_empty() {
            ALL_METRICS.iter().map(|s| s.to_string()).collect()
        } else {
            cw_config.include.into_iter().collect()
        };

        let exclude_set: HashSet<String> = cw_config.exclude.into_iter().collect();

        // Create channel for async publishing
        let (sender, receiver) = bounded::<MetricEntry>(CHANNEL_BUFFER_SIZE);

        let namespace = cw_config.namespace.clone();
        let include_filter = include_set.clone();
        let exclude_filter = exclude_set.clone();

        // Spawn background task for publishing metrics
        tokio::spawn(async move {
            while let Ok(metric) = receiver.recv_async().await {
                let metric_data =
                    build_metric_data(&metric, &dimensions, &include_filter, &exclude_filter);

                if metric_data.is_empty() {
                    continue;
                }

                if let Err(e) = client
                    .put_metric_data()
                    .namespace(&namespace)
                    .set_metric_data(Some(metric_data))
                    .send()
                    .await
                {
                    error!(error = %e, "Failed to publish metrics to CloudWatch");
                }
            }
            debug!("CloudWatch metrics publisher task exiting");
        });

        debug!(
            namespace = %cw_config.namespace,
            "CloudWatch metrics backend initialized"
        );

        Ok(Self {
            sender,
            include_set,
            exclude_set,
        })
    }

    /// Check if a metric should be included based on include/exclude filters.
    fn should_include(&self, metric_name: &str) -> bool {
        self.include_set.contains(metric_name) && !self.exclude_set.contains(metric_name)
    }
}

/// Build metric data from a MetricEntry.
fn build_metric_data(
    metric: &MetricEntry,
    dimensions: &[Dimension],
    include_set: &HashSet<String>,
    exclude_set: &HashSet<String>,
) -> Vec<MetricDatum> {
    let mut data = Vec::new();

    let should_include = |name: &str| include_set.contains(name) && !exclude_set.contains(name);

    // Helper to create a metric datum
    let create_datum = |name: &str, value: f64, unit: StandardUnit| -> MetricDatum {
        let mut builder = MetricDatum::builder()
            .metric_name(name)
            .value(value)
            .unit(unit);

        for dim in dimensions {
            builder = builder.dimensions(dim.clone());
        }

        builder.build()
    };

    if should_include("total_received") {
        data.push(create_datum(
            "total_received",
            metric.total_received as f64,
            StandardUnit::Count,
        ));
    }

    if should_include("total_completed") {
        data.push(create_datum(
            "total_completed",
            metric.total_completed as f64,
            StandardUnit::Count,
        ));
    }

    if should_include("total_process_errors") {
        data.push(create_datum(
            "total_process_errors",
            metric.total_process_errors as f64,
            StandardUnit::Count,
        ));
    }

    if should_include("total_output_errors") {
        data.push(create_datum(
            "total_output_errors",
            metric.total_output_errors as f64,
            StandardUnit::Count,
        ));
    }

    if should_include("streams_started") {
        data.push(create_datum(
            "streams_started",
            metric.streams_started as f64,
            StandardUnit::Count,
        ));
    }

    if should_include("streams_completed") {
        data.push(create_datum(
            "streams_completed",
            metric.streams_completed as f64,
            StandardUnit::Count,
        ));
    }

    if should_include("duplicates_rejected") {
        data.push(create_datum(
            "duplicates_rejected",
            metric.duplicates_rejected as f64,
            StandardUnit::Count,
        ));
    }

    if should_include("stale_entries_removed") {
        data.push(create_datum(
            "stale_entries_removed",
            metric.stale_entries_removed as f64,
            StandardUnit::Count,
        ));
    }

    if should_include("in_flight") {
        data.push(create_datum(
            "in_flight",
            metric.in_flight as f64,
            StandardUnit::Count,
        ));
    }

    if should_include("throughput_per_sec") {
        data.push(create_datum(
            "throughput_per_sec",
            metric.throughput_per_sec,
            StandardUnit::CountSecond,
        ));
    }

    if should_include("input_bytes") {
        data.push(create_datum(
            "input_bytes",
            metric.input_bytes as f64,
            StandardUnit::Bytes,
        ));
    }

    if should_include("output_bytes") {
        data.push(create_datum(
            "output_bytes",
            metric.output_bytes as f64,
            StandardUnit::Bytes,
        ));
    }

    if should_include("bytes_per_sec") {
        data.push(create_datum(
            "bytes_per_sec",
            metric.bytes_per_sec,
            StandardUnit::BytesSecond,
        ));
    }

    data
}

#[async_trait]
impl Metrics for CloudWatchMetrics {
    fn record(&mut self, metric: MetricEntry) {
        // Check if any metrics would be included
        let has_metrics = ALL_METRICS.iter().any(|name| self.should_include(name));

        if !has_metrics {
            return;
        }

        // Non-blocking send - drop metrics if channel is full
        if let Err(e) = self.sender.try_send(metric) {
            debug!(error = %e, "CloudWatch metrics channel full, dropping metric");
        }
    }
}

#[async_trait]
impl Closer for CloudWatchMetrics {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("CloudWatch metrics backend closing");
        // Dropping the sender will cause the background task to exit
        // after processing remaining messages
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_cloudwatch(conf: Value) -> Result<ExecutionType, Error> {
    Ok(ExecutionType::Metrics(Box::new(
        CloudWatchMetrics::new(conf).await?,
    )))
}

/// Registers the CloudWatch metrics plugin.
pub(crate) fn register_cloudwatch() -> Result<(), Error> {
    let config = r#"type: object
properties:
  namespace:
    type: string
    default: "Fiddler"
  region:
    type: string
  credentials:
    type: object
    properties:
      access_key_id:
        type: string
      secret_access_key:
        type: string
      session_token:
        type: string
    required:
      - access_key_id
      - secret_access_key
  include:
    type: array
    items:
      type: string
  exclude:
    type: array
    items:
      type: string
  dimensions:
    type: array
    items:
      type: object
      properties:
        name:
          type: string
        value:
          type: string
      required:
        - name
        - value"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "cloudwatch".into(),
        ItemType::Metrics,
        conf_spec,
        create_cloudwatch,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_namespace() {
        assert_eq!(default_namespace(), "Fiddler");
    }

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
namespace: "TestNamespace"
region: "us-west-2"
include:
  - total_received
  - throughput_per_sec
exclude:
  - stale_entries_removed
dimensions:
  - name: Environment
    value: production
"#;
        let config: CloudWatchConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.namespace, "TestNamespace");
        assert_eq!(config.region, Some("us-west-2".to_string()));
        assert_eq!(config.include.len(), 2);
        assert_eq!(config.exclude.len(), 1);
        assert_eq!(config.dimensions.len(), 1);
    }

    #[test]
    fn test_config_defaults() {
        let yaml = "{}";
        let config: CloudWatchConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.namespace, "Fiddler");
        assert!(config.region.is_none());
        assert!(config.credentials.is_none());
        assert!(config.include.is_empty());
        assert!(config.exclude.is_empty());
        assert!(config.dimensions.is_empty());
    }

    #[test]
    fn test_build_metric_data_all_metrics() {
        let metric = MetricEntry {
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
            input_bytes: 1000,
            output_bytes: 900,
            bytes_per_sec: 90.0,
        };

        let include_set: HashSet<String> = ALL_METRICS.iter().map(|s| s.to_string()).collect();
        let exclude_set: HashSet<String> = HashSet::new();

        let data = build_metric_data(&metric, &[], &include_set, &exclude_set);
        assert_eq!(data.len(), 13);
    }

    #[test]
    fn test_build_metric_data_with_filter() {
        let metric = MetricEntry {
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
            input_bytes: 1000,
            output_bytes: 900,
            bytes_per_sec: 90.0,
        };

        let include_set: HashSet<String> = vec![
            "total_received".to_string(),
            "throughput_per_sec".to_string(),
        ]
        .into_iter()
        .collect();
        let exclude_set: HashSet<String> = HashSet::new();

        let data = build_metric_data(&metric, &[], &include_set, &exclude_set);
        assert_eq!(data.len(), 2);
    }

    #[test]
    fn test_build_metric_data_with_exclude() {
        let metric = MetricEntry {
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
            input_bytes: 1000,
            output_bytes: 900,
            bytes_per_sec: 90.0,
        };

        let include_set: HashSet<String> = ALL_METRICS.iter().map(|s| s.to_string()).collect();
        let exclude_set: HashSet<String> = vec!["stale_entries_removed".to_string()]
            .into_iter()
            .collect();

        let data = build_metric_data(&metric, &[], &include_set, &exclude_set);
        assert_eq!(data.len(), 12);
    }

    #[test]
    fn test_build_metric_data_with_dimensions() {
        let metric = MetricEntry::default();

        let dims = vec![Dimension::builder()
            .name("Environment")
            .value("test")
            .build()];

        let include_set: HashSet<String> = vec!["total_received".to_string()].into_iter().collect();
        let exclude_set: HashSet<String> = HashSet::new();

        let data = build_metric_data(&metric, &dims, &include_set, &exclude_set);
        assert_eq!(data.len(), 1);
        // Dimensions are set on each datum
        let datum = &data[0];
        assert_eq!(datum.dimensions().len(), 1);
    }

    #[test]
    fn test_register_cloudwatch() {
        let result = register_cloudwatch();
        // May return DuplicateRegisteredName if already registered by another test
        assert!(result.is_ok() || matches!(result, Err(crate::Error::DuplicateRegisteredName(_))));
    }
}
