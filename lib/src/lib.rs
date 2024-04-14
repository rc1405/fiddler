use std::collections::HashMap;
use serde_json::Value;
use tokio::sync::oneshot;

use thiserror::Error;
pub mod config;
mod env;
pub mod modules;
use async_trait::async_trait;
pub use env::Environment;

mod macros;

#[derive(Clone, Debug, Default)]
pub struct Message {
    pub bytes: Vec<u8>,
    pub metadata: HashMap<String, Value>,
}

pub type MessageBatch = Vec<Message>;

pub type CallbackChan = oneshot::Sender<Status>;

pub fn new_callback_chan() -> (oneshot::Sender<Status>, oneshot::Receiver<Status>) {
    oneshot::channel()
}

#[derive(Clone, Debug)]
pub enum Status {
    Processed,
    Errored(Vec<String>)
}

pub trait Closer {
    fn close(&self) -> Result<(), Error>;
}

pub trait Connect {
    fn connect(&self) -> Result<(), Error>;
}

#[async_trait]
pub trait Input: Connect + Closer {
    async fn read(&self) -> Result<(Message, CallbackChan), Error>;
}

pub trait InputBatch: Connect + Closer {
    fn read_batch(&self) -> Result<(MessageBatch, CallbackChan), Error>;
}

#[async_trait]
pub trait Output: Connect + Closer {
    async fn write(&self, message: Message) -> Result<(), Error>;
}

pub trait OutputBatch: Connect + Closer {
    fn write_batch(&self, message_batch: MessageBatch) -> Result<(), Error>;
}

#[async_trait]
pub trait Processor: Closer {
    async fn process(&self, message: Message) -> Result<MessageBatch, Error>;
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("UnableToSerializeObject: {0}")]
    UnableToSerializeYamlObject(#[from] serde_yaml::Error),
    #[error("UnableToSerializeObject: {0}")]
    UnableToSerializeJsonObject(#[from] serde_json::Error),
    #[error("ValidationError: {0}")]
    Validation(String),
    #[error("ExecutionError: {0}")]
    ExecutionError(String),
    #[error("EndOfInput")]
    EndOfInput,
    #[error("InternalServerError")]
    UnableToSecureLock,
    #[error("DuplicateRegisteredName: {0}")]
    DuplicateRegisteredName(String),
    #[error("InvalidValidationSchema: {0}")]
    InvalidValidationSchema(String),
    #[error("ConfigurationValidationFailed: {0}")]
    ConfigFailedValidation(String),
    #[error("ConfigurationItemNotFound: {0}")]
    ConfigurationItemNotFound(String),
    #[error("NotYetImplemented")]
    NotYetImplemented,
    #[error("PipelineProcessingError: {0}")]
    UnableToSendToChannel(String),
    #[error("ProcessorFailure: {0}")]
    ProcessingError(String),
    #[error("ConditionalCheckfailed")]
    ConditionalCheckfailed,
    #[error("NotConnected")]
    NotConnected,
    #[error("InputError: {0}")]
    InputError(String),
    #[error("OutputError: {0}")]
    OutputError(String),
    #[error("NoInputToReturn")]
    NoInputToReturn
}
