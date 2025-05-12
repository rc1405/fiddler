use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::MessageBatch;
use crate::{Closer, Error, Processor};
use async_trait::async_trait;
use base64::{prelude::BASE64_STANDARD, Engine};
use fiddler_macros::fiddler_registration_func;
use serde::Deserialize;
use serde_yaml::Value;

#[derive(Clone, Default, Deserialize)]
pub struct DecoderConfig {
    algorithm: Option<Algoritym>,
}

#[derive(Clone, Default, Deserialize)]
pub enum Algoritym {
    #[default]
    Base64,
}

#[derive(Clone, Default)]
pub struct Decoder {
    algorithm: Algoritym,
}

#[async_trait]
impl Processor for Decoder {
    async fn process(&self, mut message: Message) -> Result<MessageBatch, Error> {
        match self.algorithm {
            Algoritym::Base64 => {
                let content = String::from_utf8(message.bytes)
                    .map_err(|e| Error::ProcessingError(format!("{e}")))?;
                let result = BASE64_STANDARD
                    .decode(content)
                    .map_err(|e| Error::ProcessingError(format!("{e}")))?;

                message.bytes = result;
                Ok(vec![message])
            }
        }
    }
}

impl Closer for Decoder {}

#[fiddler_registration_func]
fn create_decode(conf: Value) -> Result<ExecutionType, Error> {
    let mut proc = Decoder::default();
    let c: DecoderConfig = serde_yaml::from_value(conf)?;
    if let Some(a) = c.algorithm {
        proc.algorithm = a;
    };
    Ok(ExecutionType::Processor(Box::new(proc)))
}

pub(super) fn register_decode() -> Result<(), Error> {
    let config = "type: object";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "decode".into(),
        ItemType::Processor,
        conf_spec,
        create_decode,
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_decode().unwrap()
    }

    #[tokio::test]
    async fn decoded() {
        let input_str = "SGVsbG8gV29ybGQ=";
        let msg = Message {
            bytes: input_str.as_bytes().into(),
            ..Default::default()
        };
        let processor = Decoder::default();
        let output = processor.process(msg).await.unwrap();
        assert_eq!(
            output,
            vec![Message {
                bytes: "Hello World".as_bytes().into(),
                ..Default::default()
            },]
        )
    }
}
