use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::CallbackChan;
use crate::Message;
use crate::{Closer, Error, Input};
use async_std::io;
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use serde_yaml::Value;

pub struct StdIn {}

#[async_trait]
impl Input for StdIn {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        let mut buffer = String::new();
        let stdin = io::stdin();
        let _ = stdin
            .read_line(&mut buffer)
            .await
            .map_err(|_| Error::EndOfInput)?;

        // remove new line character
        let _ = buffer.pop();

        if buffer == *"exit()" {
            return Err(Error::EndOfInput);
        };

        Ok((
            Message {
                bytes: buffer.into_bytes(),
                ..Default::default()
            },
            None,
        ))
    }
}

impl Closer for StdIn {}

#[fiddler_registration_func]
fn create_stdin(_conf: Value) -> Result<ExecutionType, Error> {
    Ok(ExecutionType::Input(Box::new(StdIn {})))
}

pub(super) fn register_stdin() -> Result<(), Error> {
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
