use async_trait::async_trait;
use tokio::sync::mpsc::{Sender, Receiver, channel};

use crate::{Error, Input, Closer};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use crate::{CallbackChan, new_callback_chan, Status};
use serde_yaml::Value;
use serde::Deserialize;
use std::fs::{self, File, read_to_string};
use std::sync::mpsc::{sync_channel, SyncSender, TryRecvError};
use std::io::{prelude::*, BufReader, Seek, SeekFrom};

#[derive(Deserialize, Default)]
enum CodecType {
    #[default]
    Lines,
    ToEnd,
    Tail,
}

enum ReaderType {
    Lines(std::io::Lines<BufReader<File>>),
    ToEnd(File),
    Tail(String, u64, SyncSender<u64>),
}

#[derive(Deserialize, Default)]
struct FileReaderConfig{
    filename: String,
    codec: CodecType,
    position_filename: Option<String>,
}

pub struct FileReader {
    filename: String,
    position_filename: Option<String>,
    codec: CodecType,
    receiver: Receiver<Result<(Message, CallbackChan), Error>>,
}

async fn read_file(reader: ReaderType, sender: Sender<Result<(Message, CallbackChan), Error>>) -> Result<(), Error> {
    
    match reader {
        ReaderType::Lines(mut li) => {
            for line in li {
                match line {
                    Ok(line) => {
                        // println!("Read line");
                        let (tx, rx) = new_callback_chan();
                        tokio::spawn(rx);

                        sender.send(Ok((Message{
                            bytes: line.into_bytes(),
                            ..Default::default()
                        }, tx))).await;
                    },
                    Err(e) => {
                        sender.send(Err(Error::InputError(format!("{}", e)))).await;
                        return Ok(())
                    }
                }
            };

            sender.send(Err(Error::EndOfInput)).await;
            return Ok(())
        },
        ReaderType::ToEnd(mut f) => {
            let mut contents = String::new();
            match f.read_to_string(&mut contents) {
                Ok(_) => {},
                Err(e) => {
                    sender.send(Err(Error::InputError(format!("{}", e)))).await;
                    return Ok(())
                },
            };
            
            let (tx, rx) = new_callback_chan();
            tokio::spawn(rx);

            sender.send(Ok((Message{
                bytes: contents.into_bytes(),
                ..Default::default()
            }, tx))).await;

            sender.send(Err(Error::EndOfInput)).await;

            return Ok(());
        },
        ReaderType::Tail(filename, pos, sync) => {
            let mut file = File::open(&filename).map_err(|e| Error::InputError(format!("{}: {}", filename, e)))?;
            let mut current_pos = pos;

            let metadata = file.metadata().map_err(|e| Error::InputError(format!("{}: {}", filename, e)))?;
            if metadata.len() < pos {
                current_pos = 0;
                sync.send(current_pos).unwrap();
            };

            let _ = file.seek(SeekFrom::Start(current_pos.clone())).map_err(|e| Error::InputError(format!("{}: {}", filename, e)))?;
            let mut reader = BufReader::new(file);
            
            loop {
                let mut line = String::new();
                let len = reader.read_line(&mut line).map_err(|e| Error::InputError(format!("{}: {}", filename.clone(), e)))?;
                
                current_pos += len as u64;
    
                let s = sync.clone();
    
                if len == 0 {
                    sender.send(Err(Error::NoInputToReturn)).await;
                    continue
                };
    
                // remove new line character
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                };
    
                let (tx, rx) = new_callback_chan();
                tokio::spawn(async move {
                    match rx.await {
                        Ok(status) => {
                            if let Status::Processed = status {
                                s.send(current_pos).unwrap();
                            };
                        },
                        Err(_) => {},
                    };
                });
    
                sender.send(Ok((Message{
                    bytes: line.into_bytes(),
                    ..Default::default()
                },tx))).await;
            }
            

            return Ok(())
        }
    }



}

impl Closer for FileReader {}

#[async_trait]
impl Input for FileReader {
    async fn read(&mut self) -> Result<(Message, CallbackChan), Error> {
        // println!("reading input");
        match self.receiver.recv().await {
            Some(i) => i,
            None => Err(Error::EndOfInput),        
        }
    }
}

fn create_file(conf: &Value) -> Result<ExecutionType, Error> {
    let c: FileReaderConfig = serde_yaml::from_value(conf.clone())?;
    if let CodecType::Tail = c.codec {
        if c.position_filename.is_none() {
            return Err(Error::ConfigFailedValidation("position_file must be included with tail type".into()))
        }
    }

    let mut file = File::open(c.filename.clone()).map_err(|e| Error::InputError(format!("{}: {}", c.filename.clone(), e)))?;

    let inner = match c.codec {
        CodecType::Lines =>  {
            let reader = BufReader::new(file);
            ReaderType::Lines(reader.lines())
        },
        CodecType::ToEnd => ReaderType::ToEnd(file),
        CodecType::Tail => {
            let (sync_sender, receiver) = sync_channel(1);
            let position_file_name = c.position_filename.clone().ok_or(Error::InputError("position file must be included".into()))?;
            let position = read_to_string(position_file_name.clone()).unwrap_or("0".into());
            let mut current_position = position.parse::<u64>().unwrap_or(0);

            let metadata = file.metadata().map_err(|e| Error::InputError(format!("{}: {}", c.filename.clone(), e)))?;
            if metadata.len() < current_position {
                current_position = 0;
                fs::write(position_file_name.clone(), format!("{current_position}")).map_err(|e| Error::InputError(format!("{}: {}", c.filename, e))).unwrap();
            };

            let _ = file.seek(SeekFrom::Start(current_position)).map_err(|e| Error::InputError(format!("{}: {}", c.filename.clone(), e)))?;

            let passed_position_reader = current_position.clone();
            let filename = c.filename.clone();

            tokio::spawn(async move {
                
                loop {
                    match receiver.try_recv() {
                        Ok(msg) => {
                            if msg == 0 {
                                current_position = 0;
                                fs::write(position_file_name.clone(), format!("{current_position}")).map_err(|e| Error::InputError(format!("{}: {}", filename, e))); //.unwrap();
                            };

                            if &msg > &current_position {
                                current_position = msg;
                                fs::write(position_file_name.clone(), format!("{current_position}")).map_err(|e| Error::InputError(format!("{}: {}", filename, e))); //.unwrap();
                            };
                        },
                        Err(e) => {
                            if let TryRecvError::Empty = e {
                                std::thread::sleep(std::time::Duration::from_millis(10));
                                continue
                            } else {
                                return
                            }
                        },
                    }
                };
            });

            ReaderType::Tail(c.filename.clone(), passed_position_reader, sync_sender)
        }
    };
    
    let (sender, receiver) = channel(1000);

    tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("file_reader")
            .worker_threads(1)
            // .on_thread_start(move || set_current_thread_priority_low())
            .build()
            .expect("Creating tokio runtime");
        runtime.block_on(read_file(inner, sender))
    });
    
    Ok(ExecutionType::Input(Box::new(FileReader{
        filename: c.filename,
        position_filename: c.position_filename,
        codec: c.codec,
        receiver,
    })))
}

// #[fiddler_registration_func]
pub fn register_file() -> Result<(), Error> {
    let config = "type: object
properties:
  filename: 
    type: string
  codec:
    type: string
  position_file:
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