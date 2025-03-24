use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::MessageBatch;
use crate::{Closer, Error, Processor};
use async_trait::async_trait;
use serde_yaml::Value;

#[derive(Clone)]
pub struct Lines {}

#[async_trait]
impl Processor for Lines {
    async fn process(&self, message: Message) -> Result<MessageBatch, Error> {
        let content = String::from_utf8(message.bytes)
            .map_err(|e| Error::ProcessingError(format!("{}", e)))?;
        let output: Vec<Message> = content
            .split('\n')
            .map(|msg| Message {
                bytes: msg.as_bytes().into(),
                metadata: message.metadata.clone(),
            })
            .collect();
        Ok(output)
    }
}

impl Closer for Lines {}

fn create_lines(_conf: &Value) -> Result<ExecutionType, Error> {
    Ok(ExecutionType::Processor(Box::new(Lines {})))
}

pub (super) fn register_lines() -> Result<(), Error> {
    let config = "type: object";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("lines".into(), ItemType::Processor, conf_spec, create_lines)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_lines().unwrap()
    }
}
