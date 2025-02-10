use crate::{Error, Output, Closer};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use serde_yaml::Value;
use async_trait::async_trait;

#[derive(Clone)]
pub struct StdOut {}

#[async_trait]
impl Output for StdOut {
    async fn write(&mut self, message: Message) -> Result<(), Error> {
        let msg = String::from_utf8(message.bytes).map_err(|_| Error::EndOfInput)?;
        Ok(())
    }
}

impl Closer for StdOut {}

fn create_stdout(_conf: &Value) -> Result<ExecutionType, Error> {
    Ok(ExecutionType::Output(Box::new(StdOut{})))
}

pub fn register_stdout() -> Result<(), Error> {
    let config = "type: object";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("stdout".into(), ItemType::Output, conf_spec, create_stdout)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_stdout().unwrap()
    }
}