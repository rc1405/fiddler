use crate::{Error, Output, Closer, Connect};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use serde_yaml::Value;
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;

#[derive(Clone)]
pub struct StdOut {}

#[async_trait]
impl Output for StdOut {
    async fn write(self: &Self, message: Message) -> Result<(), Error> {
        let msg = String::from_utf8(message.bytes).map_err(|_| Error::EndOfInput)?;
        println!("echo: {}", msg);
        Ok(())
    }
}

impl Closer for StdOut {
    fn close(self: &Self) -> Result<(), Error> {
        Ok(())
    }
}

impl Connect for StdOut {
    fn connect(self: &Self) -> Result<(), Error> {
        Ok(())
    }
}

fn create_stdout(_conf: &Value) -> Result<ExecutionType, Error> {
    return Ok(ExecutionType::Output(Box::new(StdOut{})))
}

#[fiddler_registration_func]
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