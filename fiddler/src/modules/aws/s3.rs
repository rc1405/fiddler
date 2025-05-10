use super::sqs::SqsConfig;
use crate::config::{register_plugin, ConfigSpec, ExecutionType, ItemType};
use crate::Message;
use crate::Status;
use crate::{new_callback_chan, CallbackChan};
use crate::{Closer, Error, Input};
use async_trait::async_trait;
use aws_lambda_events::s3::S3Event;
use aws_sdk_s3::error::DisplayErrorContext;
use aws_sdk_s3::{client::Client, config};
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, RecvError, Sender};
use serde::Deserialize;
use serde_yaml::Value;
use std::collections::HashMap;
use tokio::io::AsyncBufReadExt;
use tokio::task::JoinSet;
use tracing::{debug, info};

enum ReaderType {
    Queue(String),
    ListObjects(String, String),
}

#[derive(Deserialize, Default, Clone)]
struct S3InputConfig {
    bucket: String,
    prefix: Option<String>,
    queue: Option<SqsConfig>,
    credentials: Option<super::Credentials>,
    delete_after_read: Option<bool>,
    endpoint_url: Option<String>,
    read_lines: Option<bool>,
    force_path_style_urls: Option<bool>,
    region: Option<String>,
}

pub struct AwsS3 {
    objects: Receiver<Result<(Message, Option<CallbackChan>), Error>>,
}

async fn process_object(
    s3_client: Client,
    bucket: String,
    key: String,
    read_lines: bool,
    stream_id: String,
    sender: Sender<Result<(Message, Option<CallbackChan>), Error>>,
) -> Result<(), Error> {
    println!("starting to download file: {} from {}", key, bucket);
    let result = s3_client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;

    println!("file downloaded");
    match result {
        Ok(res) => {
            if read_lines {
                let mut l = res.body.into_async_read().lines();
                while let Ok(Some(line)) = l.next_line().await {
                    println!("line: {}", line);
                    sender
                        .send_async(Ok((
                            Message {
                                bytes: line.into_bytes(),
                                stream_id: Some(stream_id.clone()),
                                ..Default::default()
                            },
                            None,
                        )))
                        .await
                        .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                }
            } else {
                sender
                    .send_async(Ok((
                        Message {
                            bytes: res.body.bytes().unwrap().into(),
                            stream_id: Some(stream_id.clone()),
                            ..Default::default()
                        },
                        None,
                    )))
                    .await
                    .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
            }
        }
        Err(e) => {
            sender
                .send_async(Err(Error::InputError(format!(
                    "{}",
                    DisplayErrorContext(e)
                ))))
                .await
                .map_err(|e| Error::InputError(format!("{}", e)))?;
        }
    };
    Ok(())
}

async fn list_objects(
    reader: ReaderType,
    sender: Sender<Result<(Message, Option<CallbackChan>), Error>>,
    s3_client: Client,
    sqs_client: aws_sdk_sqs::client::Client,
    read_lines: bool,
    delete_after_read: bool,
) -> Result<(), Error> {
    match reader {
        ReaderType::Queue(url) => loop {
            if sender.is_disconnected() {
                return Ok(());
            };

            let result = sqs_client
                .receive_message()
                .max_number_of_messages(1)
                .queue_url(&url)
                .wait_time_seconds(10)
                .send()
                .await
                .map_err(|e| {
                    Error::InputError(format!("{}", aws_sdk_sqs::error::DisplayErrorContext(e)))
                });

            match result {
                Ok(msg) => {
                    let mut messages: Vec<aws_sdk_sqs::types::Message> = msg.messages().into();
                    debug!("received {} messages from queue", messages.len());
                    if let Some(m) = messages.pop() {
                        if let Some(raw) = m.body() {
                            println!("{}", raw);

                            let test_record: HashMap<String, String> =
                                serde_json::from_str(raw).unwrap_or(HashMap::new());
                            if let Some(r) = test_record.get("Event") {
                                if r == "s3:TestEvent" {
                                    continue;
                                }
                                return Err(Error::InputError(format!("invalid message format")));
                            };

                            let _ = sqs_client
                                .change_message_visibility()
                                .queue_url(&url)
                                .receipt_handle(m.receipt_handle().unwrap())
                                .set_visibility_timeout(Some(43200))
                                .send()
                                .await;

                            let message_id = match m.message_id() {
                                Some(id) => id.into(),
                                None => uuid::Uuid::new_v4().to_string(),
                            };

                            let receipt_handle = m.receipt_handle().unwrap().to_string();
                            let (tx, rx) = new_callback_chan();
                            let c = sqs_client.clone();
                            let url2 = url.clone();

                            tokio::spawn(async move {
                                if let Ok(status) = rx.await {
                                    match status {
                                        Status::Processed => {
                                            c.delete_message()
                                                .receipt_handle(receipt_handle)
                                                .queue_url(&url2)
                                                .send()
                                                .await
                                                .unwrap();
                                        }
                                        Status::Errored(_e) => {
                                            c.change_message_visibility()
                                                .receipt_handle(receipt_handle)
                                                .queue_url(&url2)
                                                .visibility_timeout(10)
                                                .send()
                                                .await
                                                .unwrap();
                                        }
                                    };
                                };
                            });

                            sender
                                .send_async(Ok((
                                    Message {
                                        message_type: crate::MessageType::BeginStream(
                                            message_id.clone(),
                                        ),
                                        ..Default::default()
                                    },
                                    Some(tx),
                                )))
                                .await
                                .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;

                            let records: S3Event = serde_json::from_str(raw)?;
                            for record in records.records {
                                let object_id = uuid::Uuid::new_v4().to_string();
                                let bucket = match record.s3.bucket.name {
                                    Some(b) => b,
                                    None => continue,
                                };

                                let key = match record.s3.object.key {
                                    Some(k) => k,
                                    None => continue,
                                };

                                let (tx, rx) = new_callback_chan();

                                if delete_after_read {
                                    let s3 = s3_client.clone();
                                    let b = bucket.clone();
                                    let k = key.clone();
                                    tokio::spawn(async move {
                                        if let Ok(Status::Processed) = rx.await {
                                            s3.delete_object()
                                                .bucket(b)
                                                .key(k)
                                                .send()
                                                .await
                                                .map_err(|e| DisplayErrorContext(e))
                                                .unwrap();
                                        }
                                    });
                                } else {
                                    tokio::spawn(rx);
                                };

                                sender
                                    .send_async(Ok((
                                        Message {
                                            message_type: crate::MessageType::BeginStream(
                                                object_id.clone(),
                                            ),
                                            stream_id: Some(message_id.clone()),
                                            ..Default::default()
                                        },
                                        Some(tx),
                                    )))
                                    .await
                                    .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;

                                process_object(
                                    s3_client.clone(),
                                    bucket,
                                    key,
                                    read_lines,
                                    object_id.clone(),
                                    sender.clone(),
                                )
                                .await?;

                                sender
                                    .send_async(Ok((
                                        Message {
                                            message_type: crate::MessageType::EndStream(
                                                object_id.clone(),
                                            ),
                                            stream_id: Some(message_id.clone()),
                                            ..Default::default()
                                        },
                                        None,
                                    )))
                                    .await
                                    .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                            }

                            sender
                                .send_async(Ok((
                                    Message {
                                        message_type: crate::MessageType::EndStream(
                                            message_id.clone(),
                                        ),
                                        ..Default::default()
                                    },
                                    None,
                                )))
                                .await
                                .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                        }
                    }
                }
                Err(e) => {
                    sender
                        .send_async(Err(e))
                        .await
                        .map_err(|e| Error::InputError(format!("{}", e)))?;
                }
            };
        },
        ReaderType::ListObjects(bucket, prefix) => {
            let mut handles = JoinSet::new();

            let result = s3_client
                .list_objects()
                .bucket(&bucket)
                .prefix(&prefix)
                .send()
                .await;

            match result {
                Ok(objects) => {
                    for obj in objects.contents().to_owned() {
                        let stream_id = uuid::Uuid::new_v4().to_string();

                        let (tx, rx) = new_callback_chan();

                        let key = match obj.key {
                            Some(k) => k,
                            None => continue,
                        };

                        debug!(bucket = bucket, key = key, "processing object");

                        if delete_after_read {
                            let s3 = s3_client.clone();
                            let b = bucket.clone();
                            let k = key.clone();
                            handles.spawn(async move {
                                println!("deleting s3://{}/{}", b, k);
                                if let Ok(Status::Processed) = rx.await {
                                    s3.delete_object()
                                        .bucket(b)
                                        .key(k)
                                        .send()
                                        .await
                                        .map_err(|e| DisplayErrorContext(e))
                                        .unwrap();
                                }
                            });
                        } else {
                            handles.spawn(async move {
                                let _ = rx.await;
                            });
                        };

                        sender
                            .send_async(Ok((
                                Message {
                                    message_type: crate::MessageType::BeginStream(
                                        stream_id.clone(),
                                    ),
                                    ..Default::default()
                                },
                                Some(tx),
                            )))
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;

                        process_object(
                            s3_client.clone(),
                            bucket.clone(),
                            key,
                            read_lines,
                            stream_id.clone(),
                            sender.clone(),
                        )
                        .await?;

                        sender
                            .send_async(Ok((
                                Message {
                                    message_type: crate::MessageType::EndStream(stream_id.clone()),
                                    ..Default::default()
                                },
                                None,
                            )))
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                    }
                }
                Err(e) => {
                    sender
                        .send_async(Err(Error::InputError(format!(
                            "{}",
                            DisplayErrorContext(e)
                        ))))
                        .await
                        .map_err(|e| Error::InputError(format!("{}", e)))?;
                }
            };

            handles.join_all().await;
            return Err(Error::EndOfInput);
        }
    }
}

#[async_trait]
impl Input for AwsS3 {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        match self.objects.recv_async().await {
            Ok(m) => m,
            Err(e) => match e {
                RecvError::Disconnected => Err(Error::EndOfInput),
            },
        }
    }
}

impl Closer for AwsS3 {}

#[fiddler_registration_func]
fn create_s3in(conf: Value) -> Result<ExecutionType, Error> {
    let s3_conf: S3InputConfig = serde_yaml::from_value(conf.clone())?;
    let mut conf =
        config::Builder::default().behavior_version(config::BehaviorVersion::v2025_01_17());

    match s3_conf.credentials {
        Some(creds) => {
            let provider = config::Credentials::new(
                creds.access_key_id,
                creds.secret_access_key,
                creds.session_token,
                None,
                "fiddler",
            );
            conf = conf.credentials_provider(provider);
        }
        None => {
            let aws_cfg = aws_config::load_from_env().await;
            let provider = aws_cfg
                .credentials_provider()
                .ok_or(Error::ConfigFailedValidation(format!(
                    "could not establish creds"
                )))?;
            conf = conf.credentials_provider(provider)
        }
    };

    if let Some(endpoint_url) = s3_conf.endpoint_url {
        info!(endpoint_url = endpoint_url, "setting endpoint");
        conf = conf.endpoint_url(endpoint_url);
    };

    if let Some(path_urls) = s3_conf.force_path_style_urls {
        info!(force_path_style = path_urls, "Setting path style urls");
        conf = conf.force_path_style(path_urls);
    };

    if let Some(region) = s3_conf.region {
        conf = conf.region(config::Region::new(region))
    }

    let delete_after_read = s3_conf.delete_after_read.unwrap_or(false);
    let read_lines = s3_conf.read_lines.unwrap_or(false);

    conf = conf.disable_multi_region_access_points(true);

    let client = Client::from_conf(conf.build());
    let (sender, receiver) = bounded(0);

    let mut sqs_aws_conf = aws_sdk_sqs::config::Builder::default()
        .behavior_version(aws_sdk_sqs::config::BehaviorVersion::v2025_01_17());

    let reader_type = match s3_conf.queue {
        Some(sqs_conf) => {
            match sqs_conf.credentials {
                Some(creds) => {
                    let provider = config::Credentials::new(
                        creds.access_key_id,
                        creds.secret_access_key,
                        creds.session_token,
                        None,
                        "fiddler",
                    );
                    sqs_aws_conf = sqs_aws_conf.credentials_provider(provider);
                }
                None => {
                    let aws_cfg = aws_config::load_from_env().await;
                    let provider =
                        aws_cfg
                            .credentials_provider()
                            .ok_or(Error::ConfigFailedValidation(format!(
                                "could not establish creds"
                            )))?;
                    sqs_aws_conf = sqs_aws_conf.credentials_provider(provider)
                }
            };

            if let Some(region) = sqs_conf.region {
                sqs_aws_conf = sqs_aws_conf.region(config::Region::new(region));
            }

            if let Some(endpoint_url) = sqs_conf.endpoint_url {
                sqs_aws_conf = sqs_aws_conf.endpoint_url(endpoint_url);
            };
            ReaderType::Queue(sqs_conf.queue_url.clone())
        }
        None => {
            ReaderType::ListObjects(s3_conf.bucket.clone(), s3_conf.prefix.unwrap_or("".into()))
        }
    };

    let sqs_client = aws_sdk_sqs::client::Client::from_conf(sqs_aws_conf.build());

    tokio::spawn(list_objects(
        reader_type,
        sender,
        client.clone(),
        sqs_client,
        read_lines,
        delete_after_read,
    ));

    Ok(ExecutionType::Input(Box::new(AwsS3 { objects: receiver })))
}

pub(super) fn register_s3() -> Result<(), Error> {
    let config = "type: object
properties:
  bucket: 
    type: string
  prefix:
    type: string
  queue:
    type: object
    properties:
      queue_url: 
        type: string
      endpoint_url:
        type: string
      credentials:
        type: object
        properties:
          access_key_id:
            type: string
          secret_access_key:
            type: string
          session_token:
            type: string
        required:
          - access_key_id
          - secret_access_key
      region:
        type: string
    required:
      - queue_url
  credentials:
    type: object
    properties:
      access_key_id:
        type: string
      secret_access_key:
        type: string
      session_token:
        type: string
    required:
      - access_key_id
      - secret_access_key
  delete_after_read:
    type: boolean
  endpoint_url:
    type: string
  read_lines:
    type: boolean
  force_path_style_urls:
    type: boolean
  region:
    type: string
required:
  - bucket";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "aws_s3".into(),
        ItemType::Input,
        conf_spec.clone(),
        create_s3in,
    )
}
