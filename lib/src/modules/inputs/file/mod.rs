use async_trait::async_trait;
use tokio::sync::mpsc::{Sender, Receiver, channel};
use tokio::sync::{Barrier, Notify};

use crate::{Error, Input, Closer, Connect};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use crate::{CallbackChan, new_callback_chan, Status};
use serde_yaml::Value;
use fiddler_macros::fiddler_registration_func;
use serde::Deserialize;
use std::borrow::BorrowMut;
use std::fs::{self, File, read_to_string};
use std::io::{prelude::*, BufReader, Seek, SeekFrom};
use std::cell::RefCell;
use tokio::sync::Mutex;
use std::sync::mpsc::{sync_channel, SyncSender, TryRecvError};
use std::sync::Arc;

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
    position_file: Option<String>,
}

pub struct FileReader {
    filename: String,
    position_filename: Option<String>,
    codec: CodecType,
    notifier: Arc<Notify>,
    receiver: Mutex<Option<Receiver<Result<(Message, CallbackChan), Error>>>>,
}

async fn read_file(reader: ReaderType, barrier: Arc<Notify>, sender: Sender<Result<(Message, CallbackChan), Error>>) -> Result<(), Error> {
    
    match reader {
        ReaderType::Lines(mut li) => {
            loop {
                barrier.notified().await;

                match li.next() {
                    Some(line) => {
                        let msg = match line {
                            Ok(l) => l,
                            Err(e) => {
                                sender.send(Err(Error::InputError(format!("{}", e)))).await;
                                return Ok(())
                            },
                        };

                        let (tx, rx) = new_callback_chan();
                        tokio::spawn(rx);

                        sender.send(Ok((Message{
                            bytes: msg.into_bytes(),
                            ..Default::default()
                        }, tx))).await;
                    },
                    None => {
                        sender.send(Err(Error::EndOfInput)).await;
                        return Ok(())
                    }
                }
            }
        },
        ReaderType::ToEnd(mut f) => {
            barrier.notified().await;

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

            barrier.notified().await;

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
                barrier.notified().await;

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

#[async_trait]
impl Input for FileReader {
    async fn read(&self) -> Result<(Message, CallbackChan), Error> {
        self.notifier.notify_one();

        let mut r = self.receiver.lock().await;
        match r.as_mut() {
            Some(i) => {
                match i.recv().await {
                    Some(i) => i,
                    None => Err(Error::EndOfInput),        
                }
            },
            None => Err(Error::NotConnected),
        }
    }
}

impl Closer for FileReader {
    fn close(&self) -> Result<(), Error> {
        Ok(())
    }
}

#[async_trait]
impl Connect for FileReader {
    async fn connect(&self) -> Result<(), Error> {
        let mut file = File::open(self.filename.clone()).map_err(|e| Error::InputError(format!("{}: {}", self.filename.clone(), e)))?;

        let inner = match self.codec {
            CodecType::Lines =>  {
                let reader = BufReader::new(file);
                ReaderType::Lines(reader.lines())
            },
            CodecType::ToEnd => ReaderType::ToEnd(file),
            CodecType::Tail => {
                let (sync_sender, receiver) = sync_channel(1);
                let position_file_name = self.position_filename.clone().ok_or(Error::InputError("position file must be included".into()))?;
                let position = read_to_string(position_file_name.clone()).unwrap_or("0".into());
                let mut current_position = position.parse::<u64>().unwrap_or(0);

                let metadata = file.metadata().map_err(|e| Error::InputError(format!("{}: {}", self.filename.clone(), e)))?;
                if metadata.len() < current_position {
                    current_position = 0;
                    fs::write(position_file_name.clone(), format!("{current_position}")).map_err(|e| Error::InputError(format!("{}: {}", self.filename, e))).unwrap();
                };

                let _ = file.seek(SeekFrom::Start(current_position)).map_err(|e| Error::InputError(format!("{}: {}", self.filename.clone(), e)))?;

                let passed_position_reader = current_position.clone();
                let filename = self.filename.clone();

                tokio::spawn(async move {
                    
                    loop {
                        match receiver.try_recv() {
                            Ok(msg) => {
                                if msg == 0 {
                                    current_position = 0;
                                    fs::write(position_file_name.clone(), format!("{current_position}")).map_err(|e| Error::InputError(format!("{}: {}", filename, e))).unwrap();
                                };

                                if &msg > &current_position {
                                    current_position = msg;
                                    fs::write(position_file_name.clone(), format!("{current_position}")).map_err(|e| Error::InputError(format!("{}: {}", filename, e))).unwrap();
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

                ReaderType::Tail(self.filename.clone(), passed_position_reader, sync_sender)
            }
        };
        
        let (sender, receiver) = channel(1);
        tokio::spawn(read_file(inner, self.notifier.clone(), sender));
        
        let mut s = self.receiver.lock().await;
        s.replace(receiver);
        Ok(())
    }
}

fn create_file(conf: &Value) -> Result<ExecutionType, Error> {
    let c: FileReaderConfig = serde_yaml::from_value(conf.clone())?;
    if let CodecType::Tail = c.codec {
        if c.position_file.is_none() {
            return Err(Error::ConfigFailedValidation("position_file must be included with tail type".into()))
        }
    }

    Ok(ExecutionType::Input(Arc::new(FileReader{
        filename: c.filename,
        position_filename: c.position_file,
        codec: c.codec,
        notifier: Arc::new(Notify::new()),
        receiver: Mutex::new(None),
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