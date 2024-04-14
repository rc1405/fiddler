use async_trait::async_trait;
use fiddler::config::register_plugin;
use fiddler::config::ItemType;
use fiddler::config::{ConfigSpec, ExecutionType};
use fiddler::Message;
use fiddler::MessageBatch;
use fiddler::{Closer, Error, Processor};
use serde_yaml::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct Echo {}

#[async_trait]
impl Processor for Echo {
    async fn process(self: &Self, message: Message) -> Result<MessageBatch, Error> {
        let msg_str = String::from_utf8(message.bytes).unwrap();
        Ok(vec![Message {
            bytes: format!("echo: {}", msg_str).as_bytes().into(),
            ..Default::default()
        }])
    }
}

impl Closer for Echo {
    fn close(self: &Self) -> Result<(), Error> {
        Ok(())
    }
}

fn create_echo(_conf: &Value) -> Result<ExecutionType, Error> {
    return Ok(ExecutionType::Processor(Arc::new(Box::new(Echo {}))));
}

pub fn register_echo() -> Result<(), Error> {
    let config = "type: object";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("echo".into(), ItemType::Processor, conf_spec, create_echo)
}
