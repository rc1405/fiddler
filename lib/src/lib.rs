//! Some Crate level documentation to make the linter happy
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::oneshot;
use tokio::time::Duration;

use thiserror::Error;
pub mod config;
pub(crate) mod modules;
mod runtime;
use async_trait::async_trait;
pub use runtime::Runtime;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    Errored(Vec<String>),
}

#[async_trait]
pub trait Closer {
    async fn close(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

#[async_trait]
pub trait Input: Closer {
    async fn read(&mut self) -> Result<(Message, CallbackChan), Error>;
}

#[async_trait]
pub trait InputBatch: Closer {
    fn read_batch(&mut self) -> Result<(MessageBatch, CallbackChan), Error>;
}

#[async_trait]
pub trait Output: Closer {
    async fn write(&mut self, message: Message) -> Result<(), Error>;
}

#[async_trait]
pub trait OutputBatch: Closer {
    async fn write_batch(&mut self, message_batch: MessageBatch) -> Result<(), Error>;
    async fn batch_size(&self) -> usize;
    async fn interval(&self) -> Duration;
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
    NoInputToReturn,
}
