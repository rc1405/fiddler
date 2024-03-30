use async_std::io;
use async_trait::async_trait;
use crate::{Error, Input, Closer, Connect};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use crate::Callback;
use serde_yaml::Value;
use fiddler_macros::fiddler_registration_func;

pub struct StdIn {}

#[async_trait]
impl Input for StdIn {
    async fn read(self: &Self) -> Result<(Message, Callback), Error> {
        let mut buffer = String::new();
        let stdin = io::stdin();
        stdin.read_line(&mut buffer).await.map_err(|_| Error::EndOfInput)?;
        // remove new line character
        buffer.pop();

        if buffer == "exit()".to_string() {
            return Err(Error::EndOfInput)
        };
        Ok((Message{
            bytes: buffer.into_bytes(),
        }, handle_message))
    }
}

fn handle_message(_msg: Message) -> Result<(), Error> {
    Ok(())
}

impl Closer for StdIn {
    fn close(self: &Self) -> Result<(), Error> {
        Ok(())
    }
}

impl Connect for StdIn {
    fn connect(self: &Self) -> Result<(), Error> {
        Ok(())
    }
}

fn create_stdin(_conf: &Value) -> Result<ExecutionType, Error> {
    return Ok(ExecutionType::Input(Box::new(StdIn{})))
}

#[fiddler_registration_func]
pub fn register_stdin() -> Result<(), Error> {
    let config = "type: object";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("stdin".into(), ItemType::Input, conf_spec, create_stdin)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_stdin().unwrap()
    }
}