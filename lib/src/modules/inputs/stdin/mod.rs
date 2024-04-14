use async_std::io;
use async_trait::async_trait;
use crate::{Error, Input, Closer, Connect};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use crate::{CallbackChan, new_callback_chan};
use serde_yaml::Value;
use fiddler_macros::fiddler_registration_func;
use std::sync::Arc;

pub struct StdIn {}

#[async_trait]
impl Input for StdIn {
    async fn read(&self) -> Result<(Message, CallbackChan), Error> {
        let mut buffer = String::new();
        let stdin = io::stdin();
        stdin.read_line(&mut buffer).await.map_err(|_| Error::EndOfInput)?;
        // remove new line character
        buffer.pop();

        if buffer == *"exit()" {
            return Err(Error::EndOfInput)
        };

        let (tx, rx) = new_callback_chan();
        tokio::spawn(rx);

        Ok((Message{
            bytes: buffer.into_bytes(),
            ..Default::default()
        }, tx))
    }
}

impl Closer for StdIn {
    fn close(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl Connect for StdIn {
    fn connect(&self) -> Result<(), Error> {
        Ok(())
    }
}

fn create_stdin(_conf: &Value) -> Result<ExecutionType, Error> {
    Ok(ExecutionType::Input(Arc::new(Box::new(StdIn{}))))
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