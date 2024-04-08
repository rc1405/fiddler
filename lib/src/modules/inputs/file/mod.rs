use async_trait::async_trait;
use crate::{Error, Input, Closer, Connect};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use crate::Callback;
use serde_yaml::Value;
use fiddler_macros::fiddler_registration_func;
use serde::Deserialize;
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::cell::RefCell;
use std::sync::Mutex;

#[derive(Deserialize, Default)]
enum CodecType {
    #[default]
    Lines,
    ToEnd,
}

enum ReaderType {
    Lines(std::io::Lines<BufReader<File>>),
    ToEnd(File),
    Eof
}

#[derive(Deserialize, Default)]
struct FileReaderConfig{
    filename: String,
    codec: CodecType,
}

pub struct FileReader {
    filename: String,
    codec: CodecType,
    lines: Mutex<RefCell<Option<ReaderType>>>
}

#[async_trait]
impl Input for FileReader {
    async fn read(&self) -> Result<(Message, Callback), Error> {
        match self.lines.lock() {
            Ok(l) => {
                let mut lines = l.replace(None);
                match lines {
                    Some(ref mut  f) => {
                        match f {
                            ReaderType::Lines(li) => {
                                match li.next() {
                                    Some(line) => {
                                        let msg = line.unwrap();
                                        let _ = l.replace(lines);
                                        Ok((Message{
                                            bytes: msg.into_bytes(),
                                        }, handle_message))
                                    },
                                    None => {
                                        let _ = l.replace(lines);
                                        Err(Error::EndOfInput)
                                    }
                                }
                            },
                            ReaderType::ToEnd(f) => {
                                let mut contents = String::new();
                                f.read_to_string(&mut contents).map_err(|e| Error::InputError(format!("{}", e)))?;
                                let _ = l.replace(Some(ReaderType::Eof));
                                Ok((Message{
                                    bytes: contents.into_bytes(),
                                }, handle_message))
                            },
                            ReaderType::Eof => {
                                Err(Error::EndOfInput)
                            }
                        }
                        
                    },
                    None => {
                        let _ = l.replace(lines);
                        Err(Error::NotConnected)
                    }
                }
            },
            Err(_) => Err(Error::ExecutionError("Unable to get inner lock".into()))
        }
    }
}

fn handle_message(_msg: Message) -> Result<(), Error> {
    Ok(())
}

impl Closer for FileReader {
    fn close(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl Connect for FileReader {
    fn connect(&self) -> Result<(), Error> {
        match self.lines.lock() {
            Ok(c) => {
                let mut lines = c.borrow_mut();
                let file = File::open(self.filename.clone()).unwrap();

                let inner = match self.codec {
                    CodecType::Lines =>  {
                        let reader = BufReader::new(file);
                        ReaderType::Lines(reader.lines())
                    },
                    CodecType::ToEnd => ReaderType::ToEnd(file)
                };
                
                
                *lines = Some(inner);
            },
            Err(_) => return Err(Error::ExecutionError("Unable to get inner lock".into())),
        }
        Ok(())
    }
}

fn create_file(conf: &Value) -> Result<ExecutionType, Error> {
    let c: FileReaderConfig = serde_yaml::from_value(conf.clone())?;

    Ok(ExecutionType::Input(Box::new(FileReader{
        filename: c.filename,
        codec: c.codec,
        lines: Mutex::new(RefCell::new(None))
    })))
}

#[fiddler_registration_func]
pub fn register_file() -> Result<(), Error> {
    let config = "type: object
properties:
  filename: 
    type: string
  codec:
    type: string";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("file".into(), ItemType::Input, conf_spec, create_file)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_file().unwrap()
    }
}