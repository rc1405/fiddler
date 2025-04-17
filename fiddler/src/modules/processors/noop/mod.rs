use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::MessageBatch;
use crate::{Closer, Error, Processor};
use async_trait::async_trait;
use serde_yaml::Value;

#[derive(Clone)]
pub struct NoOp {}

#[async_trait]
impl Processor for NoOp {
    async fn process(&self, message: Message) -> Result<MessageBatch, Error> {
        Ok(vec![message])
    }
}

impl Closer for NoOp {}

fn create_noop(_conf: &Value) -> Result<ExecutionType, Error> {
    Ok(ExecutionType::Processor(Box::new(NoOp {})))
}

pub(super) fn register_noop() -> Result<(), Error> {
    let config = "type: object";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("noop".into(), ItemType::Processor, conf_spec, create_noop)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_noop().unwrap()
    }
}
