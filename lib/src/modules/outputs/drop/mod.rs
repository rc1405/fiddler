use crate::{Error, Output, Closer};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use serde_yaml::Value;
use async_trait::async_trait;

#[derive(Clone)]
pub struct StdDrop {}

impl Closer for StdDrop {}

#[async_trait]
impl Output for StdDrop {
    async fn write(&mut self, _message: Message) -> Result<(), Error> {
        Ok(())
    }
}

fn create_drop(_conf: &Value) -> Result<ExecutionType, Error> {
    Ok(ExecutionType::Output(Box::new(StdDrop{})))
}

// #[fiddler_registration_func]
pub fn register_drop() -> Result<(), Error> {
    let config = "type: object";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("drop".into(), ItemType::Output, conf_spec, create_drop)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_drop().unwrap()
    }
}