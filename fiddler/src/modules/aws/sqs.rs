use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::Status;
use crate::{new_callback_chan, CallbackChan};
use crate::{Closer, Error, Input, Output};
use async_trait::async_trait;
use aws_sdk_sqs::{client::Client, config};
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Sender};
use serde::Deserialize;
use serde_yaml::Value;
use std::collections::HashMap;
use tracing::error;

#[derive(Deserialize, Default, Clone)]
struct SqsConfig {
    queue_url: String,
    endpoint_url: Option<String>,
    credentials: Option<super::Credentials>,
    region: Option<String>,
}

/// AWS Simple Queue Service (SQS)
/// SQS is provided as both an input and an output.  example configuration:
///
/// ```yaml
/// input:
///   sqs:
///     queue_url: https://sqs.amazonaws.com/
/// num_threads: 1
/// processors: []
/// output:
///   sqs:
///     queue_url: https://sqs.amazonaws.com/
/// ```
///
/// Required IAM permissions to operate as an input:
///   - sqs:ReceiveMessage
///   - sqs:DeleteMessage
///   - sqs:ChangeMessageVisibility
///
/// Required IAM permissions to operate as an output:
///   - sqs:SendMessage
pub struct AwsSqs {
    client: Client,
    url: String,
    ack: Option<Sender<String>>,
}

#[async_trait]
impl Input for AwsSqs {
    async fn read(&mut self) -> Result<(Message, CallbackChan), Error> {
        let msg = self
            .client
            .receive_message()
            .max_number_of_messages(1)
            .queue_url(&self.url)
            .wait_time_seconds(10)
            .send()
            .await
            .map_err(|e| Error::InputError(format!("{}", e)))?;

        let mut messages: Vec<aws_sdk_sqs::types::Message> = msg.messages().into();
        match messages.pop() {
            Some(m) => {
                let (tx, rx) = new_callback_chan();
                let body = match m.body() {
                    Some(b) => b.as_bytes(),
                    None => return Err(Error::InputError("empty message body".into())),
                };

                let metadata = match m.attributes() {
                    Some(md) => {
                        let mut mp = HashMap::new();
                        let _ = md
                            .into_iter()
                            .map(|k| mp.insert(k.0.to_string(), k.1.clone().into()));
                        mp
                    }
                    None => HashMap::new(),
                };

                if let Some(s) = &self.ack {
                    let sender = s.clone();
                    if let Some(message_id) = m.receipt_handle.clone() {
                        println!("message_id: {}", message_id);
                        tokio::spawn(async move {
                            if let Ok(status) = rx.await {
                                if let Status::Processed = status {
                                    sender.send_async(message_id).await.unwrap();
                                }
                            }
                        });
                    }
                }

                return Ok((
                    Message {
                        bytes: body.into(),
                        metadata,
                    },
                    tx,
                ));
            }
            None => return Err(Error::NoInputToReturn),
        };
    }
}

#[async_trait]
impl Output for AwsSqs {
    async fn write(&mut self, message: Message) -> Result<(), Error> {
        let msg =
            String::from_utf8(message.bytes).map_err(|e| Error::OutputError(format!("{}", e)))?;

        self.client
            .send_message()
            .queue_url(&self.url)
            .message_body(msg)
            .send()
            .await
            .map_err(|e| Error::OutputError(format!("{}", e)))?;

        Ok(())
    }
}

impl Closer for AwsSqs {}

#[fiddler_registration_func]
fn create_sqsin(conf: Value) -> Result<ExecutionType, Error> {
    let sqs_conf: SqsConfig = serde_yaml::from_value(conf.clone())?;
    let mut conf =
        config::Builder::default().behavior_version(config::BehaviorVersion::v2025_01_17());

    match sqs_conf.credentials {
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

    if let Some(region) = sqs_conf.region {
        conf = conf.region(config::Region::new(region));
    }

    if let Some(endpoint_url) = sqs_conf.endpoint_url {
        conf = conf.endpoint_url(endpoint_url);
    };

    let client = Client::from_conf(conf.build());
    let (sender, receiver) = bounded(0);

    let ack_client = client.clone();
    let ack_url = sqs_conf.queue_url.clone();

    tokio::spawn(async move {
        loop {
            match receiver.recv_async().await {
                Ok(id) => {
                    if let Err(e) = ack_client
                        .delete_message()
                        .queue_url(&ack_url)
                        .receipt_handle(&id)
                        .send()
                        .await
                    {
                        error!(
                            error = format!("{}", e),
                            message_id = id,
                            "failed to acknowledge message"
                        )
                    }
                }
                Err(e) => {
                    error!(error = format!("{}", e), "error receiving message_ids");
                    return;
                }
            }
        }
    });

    Ok(ExecutionType::Input(Box::new(AwsSqs {
        client,
        url: sqs_conf.queue_url,
        ack: Some(sender),
    })))
}

#[fiddler_registration_func]
fn create_sqsout(conf: Value) -> Result<ExecutionType, Error> {
    let sqs_conf: SqsConfig = serde_yaml::from_value(conf.clone())?;
    let mut conf =
        config::Builder::default().behavior_version(config::BehaviorVersion::v2025_01_17());

    match sqs_conf.credentials {
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

    if let Some(region) = sqs_conf.region {
        conf = conf.region(config::Region::new(region));
    }

    if let Some(endpoint_url) = sqs_conf.endpoint_url {
        conf = conf.endpoint_url(endpoint_url);
    };

    let client = Client::from_conf(conf.build());
    Ok(ExecutionType::Output(Box::new(AwsSqs {
        client,
        url: sqs_conf.queue_url,
        ack: None,
    })))
}

pub(super) fn register_sqs() -> Result<(), Error> {
    let config = "type: object
properties:
  queue_url: 
    type: string
  endpoint_url:
    type: string
required:
  - queue_url";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "aws_sqs".into(),
        ItemType::Input,
        conf_spec.clone(),
        create_sqsin,
    )?;
    register_plugin("aws_sqs".into(), ItemType::Output, conf_spec, create_sqsout)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_sqs().unwrap()
    }
}
