//! Fast and flexible data stream processor written in Rust
//!
//! Provides a library for building data streaming pipelines using a
//! declarative yaml based configuration for data aggregation and
//! transformation
use async_trait::async_trait;
use serde::{Deserialize, Deserializer, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::oneshot;
use tokio::time::Duration;

/// Contains configuration and module registration primitives for module development
pub mod config;
pub use runtime::Runtime;
pub(crate) mod modules;
mod runtime;

/// Reserved message ID used internally for shutdown signaling.
/// User messages should not use this ID.
pub const SHUTDOWN_MESSAGE_ID: &str = "SHUTDOWN_SIGNAL";

/// Deserialize an optional duration from a string like "10s", "5m", "1h", etc.
pub(crate) fn deserialize_optional_duration<'de, D>(
    deserializer: D,
) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) => parse_duration::parse(&s)
            .map(Some)
            .map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

/// BatchingPolicy defines common configuration items for used in batching operations
/// such as OutputBatch modules.
///
/// # Example Configuration
///
/// ```yaml
/// output:
///   elasticsearch:
///     batch:
///       duration: 10s
///       size: 500
/// ```
#[derive(Deserialize, Default, Clone)]
pub struct BatchingPolicy {
    /// Maximum duration to wait before flushing a batch
    #[serde(default, deserialize_with = "deserialize_optional_duration")]
    pub duration: Option<Duration>,
    /// Maximum number of messages in a batch before flushing
    pub size: Option<usize>,
}

impl BatchingPolicy {
    /// Get the effective batch size, using default (500) if not specified.
    pub fn effective_size(&self) -> usize {
        self.size.unwrap_or(500)
    }

    /// Get the effective interval, using default (10 seconds) if not specified.
    pub fn effective_duration(&self) -> Duration {
        self.duration.unwrap_or_else(|| Duration::from_secs(10))
    }
}

/// MessageType is utilized by plugins to identiy which type of message are they sending
/// to the runtime.  [MessageType::Default] is utilized for processing data that will be
/// sent to their configured outputs.  [MessageType::BeginStream] and [MessageType::EndStream]
/// will not be processed by the pipeline but are utilized to logically group messages
/// together under a shared callback function.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub enum MessageType {
    #[default]
    /// Default message to be processed
    Default,
    /// Received from Input modules that indicates the start of an event stream with a shared callback
    /// This event is used for tracking state only and will not be processed
    BeginStream(String),
    /// Received from Input modules that indicates the end of an event stream with a shared callback
    /// This event is used for tracking state only and will not be processed
    EndStream(String),
}

/// Message is the uniform struct utilized within all modules of fiddler.
/// ```
/// # use fiddler::Message;
/// # use std::collections::HashMap;
/// let content = "This is a message being processed";
/// let message = Message{
///     bytes: content.as_bytes().into(),
///     ..Default::default()
/// };
/// ```
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Message {
    /// raw bytes of of the message to be collected, processed, and sent
    pub bytes: Vec<u8>,
    /// metadata about the message
    pub metadata: HashMap<String, Value>,
    /// Specifies the message type. If [MessageType::BeginStream] or [MessageType::EndStream] is
    /// provided, no processing of the event will take place, but the callback will be called
    /// when all child messages have been processed. This gives modules the ability to tier
    /// callback actions. Such as a module that receives a file from a queue and then processes
    /// all the lines in said file. Each line will be its own [Message]; where the stream
    /// callback will delete the original message once all lines are processed.
    pub message_type: MessageType,
    /// [Optional] Specifies the stream_id of the associated stream of messages with a shared callback
    pub stream_id: Option<String>,
}

/// A batch of messages being processed together.
///
/// Message batches are used by processors to emit multiple messages from a single input,
/// and by batch outputs to collect messages before writing.
///
/// # Example
/// ```
/// use fiddler::{Message, MessageBatch};
///
/// let batch: MessageBatch = vec![
///     Message { bytes: b"msg1".to_vec(), ..Default::default() },
///     Message { bytes: b"msg2".to_vec(), ..Default::default() },
/// ];
/// ```
pub type MessageBatch = Vec<Message>;

/// MetricEntry is the uniform struct utilized within metrics modules of fiddler.
/// ```
/// # use fiddler::MetricEntry;
/// let content = "This is a message being processed";
/// let metric = MetricEntry::default();
/// ```
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct MetricEntry {
    /// * `total_received` - Total messages received from input
    pub total_received: u64,
    /// * `total_completed` - Messages successfully processed through all outputs
    pub total_completed: u64,
    /// * `total_process_errors` - Messages that encountered processing errors
    pub total_process_errors: u64,
    /// * `total_output_errors` - Messages that encountered output errors
    pub total_output_errors: u64,
    /// * `total_filtered` - Messages intentionally filtered/dropped by processors
    pub total_filtered: u64,
    /// * `streams_started` - Number of streams started
    pub streams_started: u64,
    /// * `streams_completed` - Number of streams completed
    pub streams_completed: u64,
    /// * `duplicates_rejected` - Duplicate messages rejected
    pub duplicates_rejected: u64,
    /// * `stale_entries_removed` - Stale entries cleaned up
    pub stale_entries_removed: u64,
    /// * `in_flight` - Current number of messages being processed
    pub in_flight: usize,
    /// * `throughput_per_sec` - Current throughput in messages per second
    pub throughput_per_sec: f64,
    /// * `cpu_usage_percent` - System CPU usage percentage (0-100), None if not collected
    pub cpu_usage_percent: Option<f32>,
    /// * `memory_used_bytes` - System memory used in bytes, None if not collected
    pub memory_used_bytes: Option<u64>,
    /// * `memory_total_bytes` - System total memory in bytes, None if not collected
    pub memory_total_bytes: Option<u64>,
    /// * `input_bytes` - Total bytes received from input
    pub input_bytes: u64,
    /// * `output_bytes` - Total bytes written to output
    pub output_bytes: u64,
    /// * `bytes_per_sec` - Current throughput in bytes per second (based on output_bytes)
    pub bytes_per_sec: f64,
    /// * `latency_avg_ms` - Average message processing latency in milliseconds
    pub latency_avg_ms: f64,
    /// * `latency_min_ms` - Minimum message processing latency in milliseconds
    pub latency_min_ms: f64,
    /// * `latency_max_ms` - Maximum message processing latency in milliseconds
    pub latency_max_ms: f64,
}

/// Channel for sending acknowledgment status back to input modules.
///
/// Input modules can optionally provide a callback channel when emitting messages.
/// The runtime will send a [`Status`] through this channel once all processing
/// and output operations for the message (and any derived messages) are complete.
///
/// This enables input modules to implement at-least-once delivery semantics
/// by only acknowledging messages after successful processing.
///
/// # Example
/// ```
/// use fiddler::{new_callback_chan, Status};
///
/// let (tx, rx) = new_callback_chan();
/// // ... emit message with tx as callback ...
/// // Later, check the result:
/// // match rx.await {
/// //     Ok(Status::Processed) => println!("Success"),
/// //     Ok(Status::Errored(errs)) => println!("Failed: {:?}", errs),
/// //     Err(_) => println!("Channel closed"),
/// // }
/// ```
pub type CallbackChan = oneshot::Sender<Status>;

/// Helper function to generate transmitting and receiver pair that can be utilized for
/// the [crate::CallbackChan] for input modules.
pub fn new_callback_chan() -> (oneshot::Sender<Status>, oneshot::Receiver<Status>) {
    oneshot::channel()
}

/// Status returned through the [crate::CallbackChan] to input modules
#[derive(Clone, Debug)]
pub enum Status {
    /// Fully successful processed message
    Processed,
    /// Processing failed partially, or fully with a list of failures received.
    Errored(Vec<String>),
}

/// Closer trait utilized by input and output modules to optionally gracefully
/// exit upon successful processing of the pipeline
#[async_trait]
pub trait Closer {
    /// gracefully terminate resources prior to shutdown of processing pipeline
    async fn close(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

/// Input module trait to insert [crate::Message] into the processing pipeline.
#[async_trait]
pub trait Input: Closer {
    /// Read single message from the input module and expected return a tuple
    /// containing the [crate::Message] and a [crate::CallbackChan] for reporting status back.
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error>;
}

/// BatchInput module trait to insert one to many [crate::Message] into the pipeline.
///
/// Unlike the single-message [Input] trait, `InputBatch` allows modules to read multiple
/// messages at once. The callback channel applies to the entire batch - it will be called
/// when ALL messages in the batch have completed processing.
///
/// # Batch Callback Semantics
///
/// - If all messages succeed: callback receives `Status::Processed`
/// - If any messages fail: callback receives `Status::Errored(errors)` with error details
/// - Successful messages in a partial failure are still processed through the pipeline
#[async_trait]
pub trait InputBatch: Closer {
    /// Read multiple messages from the input module and expected return a tuple
    /// containing the [crate::MessageBatch] and a [crate::CallbackChan] for reporting status back.
    ///
    /// The callback applies to the entire batch and will be triggered once all messages
    /// in the batch have completed processing (or failed).
    async fn read_batch(&mut self) -> Result<(MessageBatch, Option<CallbackChan>), Error>;
}

/// Output module trait to write a single [crate::Message] to the output
#[async_trait]
pub trait Output: Closer {
    /// writes message to the output module
    async fn write(&mut self, message: Message) -> Result<(), Error>;
}

/// Batching output module utilized to write many messages to the output
/// based on batch_size and provided interval.  Defaults are `batch_size: 500`, `interval: 10 seconds`.
/// The queuing mechanism is provided by the runtime and will call `write_batch` if the batch size
/// has been reached, or the desired interval has passed, whichever comes first.
#[async_trait]
pub trait OutputBatch: Closer {
    /// Write [crate::MessageBatch] to the output in accordance with the provided batching policy
    async fn write_batch(&mut self, message_batch: MessageBatch) -> Result<(), Error>;

    /// returns the desired size of the [crate::MessageBatch] to provide the the output module
    async fn batch_size(&self) -> usize {
        500
    }

    /// returns the duration of how long the runtime should wait between sending [crate::MessageBatch] to
    /// the output.
    async fn interval(&self) -> Duration {
        Duration::from_secs(10)
    }
}

/// Processor is the processing module trait utilized to accept a single [crate::Message] and provide
/// one to many additional messages through [crate::MessageBatch]
#[async_trait]
pub trait Processor: Closer {
    /// process a givent [crate::Message] and return the transformed, one to many messages to continue
    /// on the pipeline.
    async fn process(&self, message: Message) -> Result<MessageBatch, Error>;
}

/// Trait for metrics backends.
///
/// Implementations of this trait are responsible for recording and exposing
/// metrics from the fiddler runtime. The trait is designed to be lightweight
/// and non-blocking to avoid impacting pipeline performance.
#[async_trait]
pub trait Metrics: Closer + Send {
    /// Records current metrics values to the backend.
    ///
    /// This method is called periodically by the runtime to update metrics.
    /// Implementations should be fast and non-blocking.
    ///
    /// # Arguments
    ///
    /// * `metric` - MetricEntry struct with details about metrics to be published
    fn record(&mut self, metric: MetricEntry);
}

/// Enum to capture errors occurred through the pipeline.
///
/// Uses `thiserror` for ergonomic error handling with proper `std::error::Error` implementation.
/// Errors that wrap other errors use `#[source]` or `#[from]` for proper error chaining.
#[derive(Debug, Error)]
pub enum Error {
    /// Yaml parsing errors found within the declarative language provided
    #[error("Unable to serialize YAML object")]
    UnableToSerializeYamlObject(
        #[from]
        #[source]
        serde_yaml::Error,
    ),

    /// JSON serialization is primarily utilized as a preparser to passing the declarative
    /// language to the given module by utilizing jsonschema to validate the input. This is unlikely
    /// to occur in practice since the yaml configuration object is converted to json for this step.
    #[error("Unable to serialize JSON object")]
    UnableToSerializeJsonObject(
        #[from]
        #[source]
        serde_json::Error,
    ),

    /// Validation errors discovered by the jsonschema evaluation of the declarative configuration language
    /// provided to a given module
    #[error("Validation error: {0}")]
    Validation(String),

    /// Error with the processing pipeline due to a failure of internal libraries or utilized modules
    #[error("Execution error: {0}")]
    ExecutionError(String),

    /// EndOfInput is an error enum variant to indicate that the input module has finished and will not
    /// receive additional input. This error triggers a graceful shutdown of the processing pipeline
    #[error("End of input reached")]
    EndOfInput,

    /// Unable to secure internal mutex lock
    #[error("Internal server error: unable to secure lock")]
    UnableToSecureLock,

    /// A plugin of the same category (input, processing, output) has already been provided
    #[error("Duplicate registered name: {0}")]
    DuplicateRegisteredName(String),

    /// The provided jsonschema configuration for a module is incorrect
    #[error("Invalid validation schema: {0}")]
    InvalidValidationSchema(String),

    /// Configuration provided to a module is invalid
    #[error("Configuration validation failed: {0}")]
    ConfigFailedValidation(String),

    /// Module is not registered with the runtime.
    #[error("Configuration item not found: {0}")]
    ConfigurationItemNotFound(String),

    /// Not yet implemented functionality
    #[error("Not yet implemented")]
    NotYetImplemented,

    /// Failure to send to an internal channel processing [crate::Message]s
    #[error("Pipeline processing error: {0}")]
    UnableToSendToChannel(String),

    /// Failure to receive from internal raw channel
    #[error("Channel receive error")]
    RecvChannelError(
        #[from]
        #[source]
        flume::RecvError,
    ),

    /// Processing module failed with an unrecoverable error. Processing of [crate::Message] is stopped and
    /// [crate::Status] is returned to the input module once all messages in this lineage have been processed
    #[error("Processor failure: {0}")]
    ProcessingError(String),

    /// Conditional check has failed for [crate::Message], such as use with [crate::modules::processors::switch]
    /// conditions
    #[error("Conditional check failed")]
    ConditionalCheckfailed,

    /// Error encountered while calling [crate::Input::read] on an input module
    #[error("Input error: {0}")]
    InputError(String),

    /// Error encountered while calling [crate::Output::write] or [crate::OutputBatch::write_batch] on an output module
    #[error("Output error: {0}")]
    OutputError(String),

    /// Error returned by input module to indicate there are no messages to process
    #[error("No input to return")]
    NoInputToReturn,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batching_policy_deserialize_seconds() {
        let yaml = r#"
duration: "10s"
size: 500
"#;
        let policy: BatchingPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.duration, Some(Duration::from_secs(10)));
        assert_eq!(policy.size, Some(500));
    }

    #[test]
    fn test_batching_policy_deserialize_milliseconds() {
        let yaml = r#"
duration: "100ms"
size: 100
"#;
        let policy: BatchingPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.duration, Some(Duration::from_millis(100)));
        assert_eq!(policy.size, Some(100));
    }

    #[test]
    fn test_batching_policy_deserialize_minutes() {
        let yaml = r#"
duration: "5m"
"#;
        let policy: BatchingPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.duration, Some(Duration::from_secs(300)));
        assert!(policy.size.is_none());
    }

    #[test]
    fn test_batching_policy_deserialize_complex_duration() {
        let yaml = r#"
duration: "1m 30s"
size: 1000
"#;
        let policy: BatchingPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.duration, Some(Duration::from_secs(90)));
        assert_eq!(policy.size, Some(1000));
    }

    #[test]
    fn test_batching_policy_deserialize_no_duration() {
        let yaml = r#"
size: 250
"#;
        let policy: BatchingPolicy = serde_yaml::from_str(yaml).unwrap();
        assert!(policy.duration.is_none());
        assert_eq!(policy.size, Some(250));
    }

    #[test]
    fn test_batching_policy_effective_defaults() {
        let policy = BatchingPolicy::default();
        assert_eq!(policy.effective_size(), 500);
        assert_eq!(policy.effective_duration(), Duration::from_secs(10));
    }

    #[test]
    fn test_batching_policy_effective_with_values() {
        let yaml = r#"
duration: "30s"
size: 1000
"#;
        let policy: BatchingPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.effective_size(), 1000);
        assert_eq!(policy.effective_duration(), Duration::from_secs(30));
    }
}
