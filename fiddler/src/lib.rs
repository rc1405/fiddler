//! Fast and flexible data stream processor written in Rust
//!
//! Provides a library for building data streaming pipelines using a
//! declaritive yaml based configuration for data aggregation and
//! transformation
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
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

/// BatchingPolicy defines common configuration items for used in batching operations
/// such as OutputBatch modules
#[derive(Deserialize, Default, Clone)]
pub struct BatchingPolicy {
    duration: Option<Duration>,
    size: Option<usize>,
}

/// MessageType is utilized by plugins to identiy which type of message are they sending
/// to the runtime.  [MessageType::Default] is utilized for processing data that will be
/// sent to their configured outputs.  [MessageType::BeginStream] and [MessageType::EndStream]
/// will not be processed by the pipeline but are utilized to logically group messages
/// together under a shared callback function.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
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
/// /// ```
/// # use fiddler:Message;
/// # use std::collections::HashMap;
/// let content = "This is a message being processed";
/// let message = Message{
///     bytes: content.as_bytes(),
///     metadata: HashMap::new(),
/// }
/// ```
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Message {
    /// raw bytes of of the message to be collected, processed, and sent
    pub bytes: Vec<u8>,
    /// metadata about the message
    pub metadata: HashMap<String, Value>,
    /// Specified message types.  If [MessageType::Parent] is provided, no processing of the event
    /// will take place, but the callback will be called when all child messages
    /// have been processed.  This gives modules the ability to tier callback actions.  Such as a
    /// module that receives a file from queue and then processes all the lines in said file.
    /// Each line will be it's own [Message]; where the parent callback will delete the message
    /// once all lines are processed
    pub message_type: MessageType,
    /// [Optional] Specifies the parentID
    pub stream_id: Option<String>,
}

/// Type alias for Vec<[crate::Message]>, a grouping of Messages being produced
pub type MessageBatch = Vec<Message>;

/// Acknowledgement channel utilized to send feedback to input module on the successful
/// or unsuccessful processing of event emited by input.
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

/// BatchInput module trait to insert one to may [crate::Message] into the pipeline.
/// InputBatch is currently not yet introduced into the runtime.
#[async_trait]
pub trait InputBatch: Closer {
    /// Read multiple messages from the input module and expected return a tuple
    /// containing the [crate::MessageBatch] and a [crate::CallbackChan] for reporting status back.
    fn read_batch(&mut self) -> Result<(MessageBatch, Option<CallbackChan>), Error>;
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

/// Enum to capture errors occured through the pipeline
#[derive(Debug, Error)]
pub enum Error {
    /// Yaml parsing errors found within the declarative language proved
    #[error("UnableToSerializeObject: {0}")]
    UnableToSerializeYamlObject(#[from] serde_yaml::Error),

    /// JSON serialization is primarily utilized as a preparser to passing the declarative
    /// language to the given module by utilizing jsonschema to validate the input.  This is unlikely
    /// to occur in practice since the yaml configuration object is converted to json for this step.
    #[error("UnableToSerializeObject: {0}")]
    UnableToSerializeJsonObject(#[from] serde_json::Error),

    /// Validation errors discovered by the jsonschema evaluation of the declarative configuration language
    /// provided to a given module
    #[error("ValidationError: {0}")]
    Validation(String),

    /// Error with the processing pipeline due to a failure of internal libraries or utilized modules
    #[error("ExecutionError: {0}")]
    ExecutionError(String),

    /// EndOfInput is a error enum variant to indicate that the input module has finished and will not
    /// receive additional input.  This error triggers a graceful shutdown of the processing pipeline
    #[error("EndOfInput")]
    EndOfInput,

    /// Unable to secure internal mutex lock
    #[error("InternalServerError")]
    UnableToSecureLock,

    /// A plugin of the same category (input, processing, output) has already been provided
    #[error("DuplicateRegisteredName: {0}")]
    DuplicateRegisteredName(String),

    /// The provided jsonschema configuration for a module in incorrect
    #[error("InvalidValidationSchema: {0}")]
    InvalidValidationSchema(String),

    /// Configuration provided to a module is invalid
    #[error("ConfigurationValidationFailed: {0}")]
    ConfigFailedValidation(String),

    /// Module is not registered with the runtime.
    #[error("ConfigurationItemNotFound: {0}")]
    ConfigurationItemNotFound(String),

    /// Not yet implemented functionality
    #[error("NotYetImplemented")]
    NotYetImplemented,

    /// Failure to send to an internal channel processing [crate::Message]s
    #[error("PipelineProcessingError: {0}")]
    UnableToSendToChannel(String),

    /// Failure to receive from internal raw channel
    #[error("ChannelRecvError: {0}")]
    RecvChannelError(#[from] flume::RecvError),

    /// Processing module failed with an unrecoverable error.  Processing of [crate::Message] is stopped and
    /// [crate::Status] is returned to the input module once all messages in this lineage have been processed
    #[error("ProcessorFailure: {0}")]
    ProcessingError(String),

    /// Conditional check has failed for [crate::Message], such as use with [crate::modules::processors::switch]
    /// conditions
    #[error("ConditionalCheckfailed")]
    ConditionalCheckfailed,

    /// Error encountered while calling [crate::Input::read] on an input module
    #[error("InputError: {0}")]
    InputError(String),

    /// Error encountered while calling [crate::Output::write] or [crate::OutputBatch::write_batch] on an output module
    #[error("OutputError: {0}")]
    OutputError(String),

    /// Error returned by input module to indicate there are no messages to process
    #[error("NoInputToReturn")]
    NoInputToReturn,
}
