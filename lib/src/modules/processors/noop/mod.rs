use crate::{Error, Processor, Closer};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use crate::MessageBatch;
use serde_yaml::Value;
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;

#[derive(Clone)]
pub struct NoOp {}

#[async_trait]
impl Processor for NoOp {
    async fn process(self: &Self, message: Message) -> Result<MessageBatch, Error> {
        Ok(vec![message])
    }
}

impl Closer for NoOp {
    fn close(self: &Self) -> Result<(), Error> {
        Ok(())
    }
}

fn create_noop(_conf: &Value) -> Result<ExecutionType, Error> {
    return Ok(ExecutionType::Processor(Box::new(NoOp{})))
}

#[fiddler_registration_func]
pub fn register_noop() -> Result<(), Error> {
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