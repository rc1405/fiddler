#![allow(clippy::needless_return)]
use async_trait::async_trait;
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::{new_callback_chan, CallbackChan, Status};
use crate::{Closer, Error, Input};
use serde::Deserialize;
use serde_yaml::Value;
use std::fs::{self, read_to_string, File};
use std::io::{prelude::*, BufReader, SeekFrom};
use flume::{bounded, Receiver, Sender, RecvError};
use tokio::time::{sleep, Duration};
use tracing::error;

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
    Tail(String, u64, Sender<u64>),
}

#[derive(Deserialize, Default)]
struct FileReaderConfig {
    filename: String,
    codec: CodecType,
    position_filename: Option<String>,
}

/// stuff and things
/// ```yaml
/// input:
///   file:
///   filename: tests{MAIN_SEPARATOR_STR}data{MAIN_SEPARATOR_STR}input.txt
///   codec: Lines
/// num_threads: 1
/// processors:
/// - label: my_cool_mapping
///   noop: {{}}
/// output:
/// validate:
///   expected:
///     - Hello World
///     - This is the end
/// ```
pub struct FileReader {
    receiver: Receiver<Result<(Message, CallbackChan), Error>>,
}

async fn read_file(
    reader: ReaderType,
    sender: Sender<Result<(Message, CallbackChan), Error>>,
) -> Result<(), Error> {
    match reader {
        ReaderType::Lines(li) => {
            for line in li {
                match line {
                    Ok(line) => {
                        let (tx, rx) = new_callback_chan();
                        tokio::spawn(rx);

                        sender
                            .send_async(Ok((
                                Message {
                                    bytes: line.into_bytes(),
                                    ..Default::default()
                                },
                                tx,
                            )))
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                    }
                    Err(e) => {
                        sender
                            .send_async(Err(Error::InputError(format!("{}", e))))
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                        return Ok(());
                    }
                }
            }

            sender.send_async(Err(Error::EndOfInput))
                .await
                .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
            return Ok(());
        }
        ReaderType::ToEnd(mut f) => {
            let mut contents = String::new();
            match f.read_to_string(&mut contents) {
                Ok(_) => {}
                Err(e) => {
                    sender
                        .send_async(Err(Error::InputError(format!("{}", e))))
                        .await
                        .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                    return Ok(());
                }
            };

            let (tx, rx) = new_callback_chan();
            tokio::spawn(rx);

            sender
                .send_async(Ok((
                    Message {
                        bytes: contents.into_bytes(),
                        ..Default::default()
                    },
                    tx,
                )))
                .await
                .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;

            sender.send_async(Err(Error::EndOfInput))
                .await
                .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;

            return Ok(());
        }
        ReaderType::Tail(filename, pos, sync) => {
            let mut file = File::open(&filename)
                .map_err(|e| Error::InputError(format!("{}: {}", filename, e)))?;
            let mut current_pos = pos;

            let metadata = file
                .metadata()
                .map_err(|e| Error::InputError(format!("{}: {}", filename, e)))?;
            if metadata.len() < pos {
                current_pos = 0;
                sync.send_async(current_pos)
                    .await
                    .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
            };

            let _ = file
                .seek(SeekFrom::Start(current_pos))
                .map_err(|e| Error::InputError(format!("{}: {}", filename, e)))?;

            let mut reader = BufReader::new(file);

            loop {
                let mut line = String::new();
                let len = reader
                    .read_line(&mut line)
                    .map_err(|e| Error::InputError(format!("{}: {}", filename.clone(), e)))?;

                current_pos += len as u64;

                let s = sync.clone();

                if len == 0 {
                    sender.send_async(Err(Error::NoInputToReturn))
                        .await
                        .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                    continue;
                };

                // remove new line character
                if line.ends_with('\n') {
                    let _ = line.pop();
                    if line.ends_with('\r') {
                        let _ = line.pop();
                    }
                };

                let (tx, rx) = new_callback_chan();
                tokio::spawn(async move {
                    if let Ok(status) = rx.await  {
                        if let Status::Processed = status {
                            #[allow(clippy::unwrap_used)]
                            let _ = s.send_async(current_pos).await.unwrap();
                        };
                    };
                });

                sender
                    .send_async(Ok((
                        Message {
                            bytes: line.into_bytes(),
                            ..Default::default()
                        },
                        tx,
                    )))
                    .await
                    .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
            }
        }
    }
}

impl Closer for FileReader {}

#[async_trait]
impl Input for FileReader {
    async fn read(&mut self) -> Result<(Message, CallbackChan), Error> {
        match self.receiver.recv_async().await {
            Ok(i) => i,
            Err(_e) => Err(Error::EndOfInput),
        }
    }
}

fn create_file(conf: &Value) -> Result<ExecutionType, Error> {
    let c: FileReaderConfig = serde_yaml::from_value(conf.clone())?;
    if let CodecType::Tail = c.codec {
        if c.position_filename.is_none() {
            return Err(Error::ConfigFailedValidation(
                "position_filename must be included with tail type".into(),
            ));
        }
    }

    let mut file = File::open(c.filename.clone())
        .map_err(|e| Error::InputError(format!("{}: {}", c.filename.clone(), e)))?;

    let inner = match c.codec {
        CodecType::Lines => {
            let reader = BufReader::new(file);
            ReaderType::Lines(reader.lines())
        }
        CodecType::ToEnd => ReaderType::ToEnd(file),
        CodecType::Tail => {
            let (sync_sender, receiver) = bounded(0);
            let position_file_name = c
                .position_filename
                .clone()
                .ok_or(Error::InputError("position file must be included".into()))?;
            let position = read_to_string(position_file_name.clone()).unwrap_or("0".into());
            let mut current_position = position.parse::<u64>().unwrap_or(0);

            let metadata = file
                .metadata()
                .map_err(|e| Error::InputError(format!("{}: {}", c.filename.clone(), e)))?;
            if metadata.len() < current_position {
                current_position = 0;
                fs::write(position_file_name.clone(), format!("{current_position}"))
                    .map_err(|e| Error::InputError(format!("{}: {}", c.filename, e)))?;
            };

            let _ = file
                .seek(SeekFrom::Start(current_position))
                .map_err(|e| Error::InputError(format!("{}: {}", c.filename.clone(), e)))?;

            let passed_position_reader = current_position;
            let filename = c.filename.clone();

            tokio::spawn(async move {
                loop {
                    match receiver.recv_async().await {
                        Ok(msg) => {
                            if msg == 0 {
                                current_position = 0;
                                fs::write(
                                    position_file_name.clone(),
                                    format!("{current_position}"),
                                )
                                .map_err(|e| Error::InputError(format!("{}: {}", filename, e)))
                                .unwrap();
                            };

                            if msg > current_position {
                                current_position = msg;
                                #[allow(clippy::unwrap_used)]
                                fs::write(
                                    position_file_name.clone(),
                                    format!("{current_position}"),
                                )
                                .unwrap();
                            };
                        }
                        Err(e) => {
                            error!(error = format!("{}", e), "closing file state handler");
                            return;
                        }
                    }
                }
            });

            ReaderType::Tail(c.filename.clone(), passed_position_reader, sync_sender)
        }
    };

    let (sender, receiver) = bounded(0);

    tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("file_reader")
            .worker_threads(1)
            // .on_thread_start(move || set_current_thread_priority_low())
            .build()
            .expect("Creating tokio runtime");
        runtime.block_on(read_file(inner, sender)).unwrap()
    });

    Ok(ExecutionType::Input(Box::new(FileReader { receiver })))
}

pub(super) fn register_file() -> Result<(), Error> {
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
