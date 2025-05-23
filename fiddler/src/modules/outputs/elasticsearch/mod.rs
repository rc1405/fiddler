use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::{BatchingPolicy, MessageBatch};
use crate::{Closer, Error, OutputBatch};
use async_trait::async_trait;
use chrono::Datelike;
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, Sender};
use serde::Deserialize;
use serde_yaml::Value;
use std::time::Duration;
use tracing::debug;

use elasticsearch::{
    auth::Credentials,
    http::{
        transport::{SingleNodeConnectionPool, Transport, TransportBuilder},
        Url,
    },
    BulkOperation, BulkParts, Elasticsearch,
};

use chrono::Utc;
use elasticsearch::cert::CertificateValidation;

#[derive(Deserialize, Default, Clone)]
pub enum CertValidation {
    #[default]
    Default,
    None,
}

#[derive(Deserialize, Default)]
struct ElasticConfig {
    url: Option<String>,
    username: Option<String>,
    password: Option<String>,
    cloud_id: Option<String>,
    index: String,
    cert_validation: Option<CertValidation>,
    batch_policy: Option<BatchingPolicy>,
}

pub struct Elastic {
    sender: Sender<Request>,
    size: usize,
    duration: Duration,
}

struct Request {
    message: MessageBatch,
    output: Sender<Result<(), Error>>,
}

impl ElasticConfig {
    fn get_client(&self) -> Result<Elasticsearch, Error> {
        let cert_validation = match self
            .cert_validation
            .clone()
            .unwrap_or(CertValidation::Default)
        {
            CertValidation::None => CertificateValidation::None,
            _ => CertificateValidation::Default,
        };

        if self.cloud_id.is_some() {
            #[allow(clippy::unwrap_used)]
            let cloud_id = self.cloud_id.clone().unwrap();
            let username = self
                .username
                .clone()
                .ok_or(Error::ConfigFailedValidation("username is required".into()))?;
            let password = self
                .password
                .clone()
                .ok_or(Error::ConfigFailedValidation("password is required".into()))?;
            let credentials = Credentials::Basic(username, password);
            let transport = Transport::cloud(&cloud_id, credentials)
                .map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
            Ok(Elasticsearch::new(transport))
        } else if self.username.is_some() {
            let url = self
                .url
                .clone()
                .ok_or(Error::ConfigFailedValidation("url is required".into()))?;
            let es_url =
                Url::parse(&url).map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
            let connection_pool = SingleNodeConnectionPool::new(es_url);
            let username = self
                .username
                .clone()
                .ok_or(Error::ConfigFailedValidation("username is required".into()))?;
            let password = self
                .password
                .clone()
                .ok_or(Error::ConfigFailedValidation("password is required".into()))?;
            let credentials = Credentials::Basic(username, password);
            let transport = TransportBuilder::new(connection_pool)
                .auth(credentials)
                .cert_validation(cert_validation)
                .build()
                .map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
            Ok(Elasticsearch::new(transport))
        } else if self.url.is_some() {
            let url = self
                .url
                .clone()
                .ok_or(Error::ConfigFailedValidation("url is required".into()))?;
            let es_url =
                Url::parse(&url).map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
            let connection_pool = SingleNodeConnectionPool::new(es_url);
            let transport = TransportBuilder::new(connection_pool)
                .cert_validation(cert_validation)
                .build()
                .map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
            Ok(Elasticsearch::new(transport))
        } else {
            Err(Error::ConfigFailedValidation(
                "unable to determine connection type".into(),
            ))
        }
    }
}

async fn elasticsearch_handler(
    es_client: Elasticsearch,
    index: String,
    requests: Receiver<Request>,
) -> Result<(), Error> {
    while let Ok(req) = requests.recv_async().await {
        let mut body: Vec<BulkOperation<_>> = Vec::new();
        let now = Utc::now();
        let index_date = format!("{}-{}-{}-{}", index, now.year(), now.month(), now.day());

        for msg in req.message {
            let v: serde_json::Value = match serde_json::from_slice(&msg.bytes) {
                Ok(i) => i,
                Err(_e) => continue,
            };

            body.push(BulkOperation::index(v).into());
        }
        let response = match es_client
            .bulk(BulkParts::Index(&index_date))
            .body(body)
            .send()
            .await
        {
            Ok(i) => i,
            Err(e) => {
                req.output
                    .send_async(Err(Error::OutputError(format!("{}", e))))
                    .await
                    .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                continue;
            }
        };

        let json: serde_json::Value = match response.json().await {
            Ok(i) => i,
            Err(e) => {
                req.output
                    .send_async(Err(Error::OutputError(format!("{}", e))))
                    .await
                    .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                continue;
            }
        };

        match json["errors"].as_bool() {
            Some(_e) => match json["items"].as_array() {
                Some(arr) => {
                    let failed: Vec<String> = arr
                        .iter()
                        .filter(|v| !v["error"].is_null())
                        .map(|v| format!("{}", v["error"]))
                        .collect();

                    if !failed.is_empty() {
                        req.output
                            .send_async(Err(Error::OutputError(format!(
                                "failed to insert record: {}",
                                failed.join(",")
                            ))))
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                        continue;
                    }
                }
                None => {
                    req.output
                        .send_async(Err(Error::OutputError(
                            "unable to deteremine result".into(),
                        )))
                        .await
                        .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                    continue;
                }
            },
            None => {
                req.output
                    .send_async(Err(Error::OutputError(
                        "unable to deteremine result".into(),
                    )))
                    .await
                    .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                continue;
            }
        };

        // use req.output.is_closed();
        req.output
            .send_async(Ok(()))
            .await
            .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
    }
    Ok(())
}

#[async_trait]
impl OutputBatch for Elastic {
    async fn write_batch(&mut self, message: MessageBatch) -> Result<(), Error> {
        debug!("Received batch, sending");
        let (tx, rx) = bounded(0);
        self.sender
            .send_async(Request {
                message,
                output: tx,
            })
            .await
            .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;

        debug!("Waiting for results");
        rx.recv_async().await??;
        debug!("Done sending details");
        Ok(())
    }

    async fn batch_size(&self) -> usize {
        self.size
    }

    async fn interval(&self) -> Duration {
        self.duration
    }
}

#[async_trait]
impl Closer for Elastic {}

#[fiddler_registration_func]
fn create_elasticsearch(conf: Value) -> Result<ExecutionType, Error> {
    let elastic: ElasticConfig = serde_yaml::from_value(conf.clone())?;

    if elastic.username.is_none() && elastic.password.is_some() {
        return Err(Error::ConfigFailedValidation(
            "password is set but username is not".into(),
        ));
    } else if elastic.username.is_some() && elastic.password.is_none() {
        return Err(Error::ConfigFailedValidation(
            "username is set but password is not".into(),
        ));
    } else if elastic.cloud_id.is_some()
        && (elastic.username.is_none() || elastic.password.is_none())
    {
        return Err(Error::ConfigFailedValidation(
            "cloud_id is set but username and/or password are not".into(),
        ));
    } else if elastic.cloud_id.is_none() && elastic.url.is_none() {
        return Err(Error::ConfigFailedValidation(
            "cloud_id or url is required".into(),
        ));
    }

    let c = elastic.get_client()?;
    let (sender, receiver) = bounded(0);
    let _ = tokio::spawn(elasticsearch_handler(c, elastic.index.clone(), receiver));
    let size = match &elastic.batch_policy {
        Some(i) => i.size.unwrap_or(500),
        None => 500,
    };

    let duration = match &elastic.batch_policy {
        Some(i) => i.duration.unwrap_or(Duration::from_secs(10)),
        None => Duration::from_secs(10),
    };
    Ok(ExecutionType::OutputBatch(Box::new(Elastic {
        sender,
        size,
        duration,
    })))
}

// #[cfg_attr(feature = "elasticsearch", fiddler_registration_func)]
pub(super) fn register_elasticsearch() -> Result<(), Error> {
    let config = "type: object
properties:
  url:
    type: string
  username:
    type: string
  password:
    type: string
  cloud_id:
    type: string
  index:
    type: string
  cert_validation:
    type: string
    enum: 
      - Default
      - None
required:
  - index
  - url";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "elasticsearch".into(),
        ItemType::OutputBatch,
        conf_spec,
        create_elasticsearch,
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_elasticsearch().unwrap()
    }
}
