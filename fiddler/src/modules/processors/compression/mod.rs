use std::io::Read;

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::MessageBatch;
use crate::{Closer, Error, Processor};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use flate2::{read, Compression};
use serde::Deserialize;
use serde_yaml::Value;

#[derive(Deserialize)]
struct CompressConfig {
    algorithm: Option<Algorithm>,
}

#[derive(Clone, Default, Deserialize)]
pub enum Algorithm {
    #[default]
    Gzip,
    Zip,
    Zlib,
}

#[derive(Clone, Default, Deserialize)]
pub enum Operation {
    Compress,
    #[default]
    Decompress,
}

#[derive(Clone, Default)]
pub struct Compress {
    algorithm: Algorithm,
    method: Operation,
}

#[async_trait]
impl Processor for Compress {
    async fn process(&self, mut message: Message) -> Result<MessageBatch, Error> {
        match self.method {
            Operation::Compress => match self.algorithm {
                Algorithm::Gzip => {
                    let mut output = Vec::new();
                    let mut deflater =
                        read::GzEncoder::new(&message.bytes[..], Compression::best());
                    deflater
                        .read_to_end(&mut output)
                        .map_err(|e| Error::ProcessingError(format!("{e}")))?;
                    message.bytes = output;
                    Ok(vec![message])
                }
                Algorithm::Zip => {
                    let mut output = Vec::new();
                    let mut deflater =
                        read::DeflateEncoder::new(&message.bytes[..], Compression::best());
                    deflater
                        .read_to_end(&mut output)
                        .map_err(|e| Error::ProcessingError(format!("{e}")))?;
                    message.bytes = output;
                    Ok(vec![message])
                }
                Algorithm::Zlib => {
                    let mut output = Vec::new();
                    let mut deflater =
                        read::ZlibEncoder::new(&message.bytes[..], Compression::best());
                    deflater
                        .read_to_end(&mut output)
                        .map_err(|e| Error::ProcessingError(format!("{e}")))?;
                    message.bytes = output;
                    Ok(vec![message])
                }
            },
            Operation::Decompress => match self.algorithm {
                Algorithm::Gzip => {
                    let mut output = Vec::new();
                    let mut deflater = read::GzDecoder::new(&message.bytes[..]);
                    deflater
                        .read_to_end(&mut output)
                        .map_err(|e| Error::ProcessingError(format!("{e}")))?;
                    message.bytes = output;
                    Ok(vec![message])
                }
                Algorithm::Zip => {
                    let mut output = Vec::new();
                    let mut deflater = read::DeflateDecoder::new(&message.bytes[..]);
                    deflater
                        .read_to_end(&mut output)
                        .map_err(|e| Error::ProcessingError(format!("{e}")))?;
                    message.bytes = output;
                    Ok(vec![message])
                }
                Algorithm::Zlib => {
                    let mut output = Vec::new();
                    let mut deflater = read::ZlibDecoder::new(&message.bytes[..]);
                    deflater
                        .read_to_end(&mut output)
                        .map_err(|e| Error::ProcessingError(format!("{e}")))?;
                    message.bytes = output;
                    Ok(vec![message])
                }
            },
        }
    }
}

impl Closer for Compress {}

#[fiddler_registration_func]
fn create_compress(conf: Value) -> Result<ExecutionType, Error> {
    let mut proc = Compress::default();
    let c: CompressConfig = serde_yaml::from_value(conf)?;
    if let Some(a) = c.algorithm {
        proc.algorithm = a;
    };
    proc.method = Operation::Compress;

    Ok(ExecutionType::Processor(Box::new(proc)))
}

#[fiddler_registration_func]
fn create_decompress(conf: Value) -> Result<ExecutionType, Error> {
    let mut proc = Compress::default();
    let c: CompressConfig = serde_yaml::from_value(conf)?;
    if let Some(a) = c.algorithm {
        proc.algorithm = a;
    };
    proc.method = Operation::Decompress;

    Ok(ExecutionType::Processor(Box::new(proc)))
}

pub(super) fn register_compress() -> Result<(), Error> {
    let config = "type: object
properties:
  algorithm: 
    type: string";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "compress".into(),
        ItemType::Processor,
        conf_spec.clone(),
        create_compress,
    )?;
    register_plugin(
        "decompress".into(),
        ItemType::Processor,
        conf_spec,
        create_decompress,
    )
}

#[cfg(test)]
mod test {
    use super::*;
    use base64::{prelude::BASE64_STANDARD, Engine};

    const DEFLATED_HELLO: &str = "80jNyclXCM8vykkBAA==";
    const GZIPPED_HELLO: &str = "H4sIAAAAAAAC//NIzcnJVwjPL8pJAQBWsRdKCwAAAA==";
    const ZLIB_HELLO: &str = "eNrzSM3JyVcIzy/KSQEAGAsEHQ==";

    #[test]
    fn register_plugin() {
        register_compress().unwrap()
    }

    #[tokio::test]
    async fn deflate_compress() {
        let input_str = "Hello World";
        let msg = Message {
            bytes: input_str.as_bytes().into(),
            ..Default::default()
        };
        let processor = Compress {
            algorithm: Algorithm::Zip,
            method: Operation::Compress,
        };
        let output = processor.process(msg).await.unwrap();
        let expected = BASE64_STANDARD.decode(DEFLATED_HELLO).unwrap();
        assert_eq!(
            output,
            vec![Message {
                bytes: expected,
                ..Default::default()
            },]
        )
    }

    #[tokio::test]
    async fn deflate_uncompress() {
        let msg = Message {
            bytes: BASE64_STANDARD.decode(DEFLATED_HELLO).unwrap(),
            ..Default::default()
        };
        let processor = Compress {
            algorithm: Algorithm::Zip,
            method: Operation::Decompress,
        };
        let output = processor.process(msg).await.unwrap();
        let expected_str = "Hello World";
        assert_eq!(
            output,
            vec![Message {
                bytes: expected_str.as_bytes().into(),
                ..Default::default()
            },]
        )
    }

    #[tokio::test]
    async fn gzip_compress() {
        let input_str = "Hello World";
        let msg = Message {
            bytes: input_str.as_bytes().into(),
            ..Default::default()
        };
        let processor = Compress {
            algorithm: Algorithm::Gzip,
            method: Operation::Compress,
        };
        let output = processor.process(msg).await.unwrap();
        let expected = BASE64_STANDARD.decode(GZIPPED_HELLO).unwrap();
        assert_eq!(
            output,
            vec![Message {
                bytes: expected,
                ..Default::default()
            },]
        )
    }

    #[tokio::test]
    async fn gzip_uncompress() {
        let msg = Message {
            bytes: BASE64_STANDARD.decode(GZIPPED_HELLO).unwrap(),
            ..Default::default()
        };
        let processor = Compress {
            algorithm: Algorithm::Gzip,
            method: Operation::Decompress,
        };
        let output = processor.process(msg).await.unwrap();
        let expected_str = "Hello World";
        assert_eq!(
            output,
            vec![Message {
                bytes: expected_str.as_bytes().into(),
                ..Default::default()
            },]
        )
    }

    #[tokio::test]
    async fn zlib_compress() {
        let input_str = "Hello World";
        let msg = Message {
            bytes: input_str.as_bytes().into(),
            ..Default::default()
        };
        let processor = Compress {
            algorithm: Algorithm::Zlib,
            method: Operation::Compress,
        };
        let output = processor.process(msg).await.unwrap();
        let expected = BASE64_STANDARD.decode(ZLIB_HELLO).unwrap();
        assert_eq!(
            output,
            vec![Message {
                bytes: expected,
                ..Default::default()
            },]
        )
    }

    #[tokio::test]
    async fn zlib_uncompress() {
        let msg = Message {
            bytes: BASE64_STANDARD.decode(ZLIB_HELLO).unwrap(),
            ..Default::default()
        };
        let processor = Compress {
            algorithm: Algorithm::Zlib,
            method: Operation::Decompress,
        };
        let output = processor.process(msg).await.unwrap();
        let expected_str = "Hello World";
        assert_eq!(
            output,
            vec![Message {
                bytes: expected_str.as_bytes().into(),
                ..Default::default()
            },]
        )
    }
}
