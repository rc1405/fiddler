use crate::{Error, Output, Closer, Connect};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use serde_yaml::Value;
use async_trait::async_trait;
use serde::Deserialize;
use fiddler_macros::fiddler_registration_func;
use std::cell::RefCell;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::pin::Pin;
use tokio::sync::mpsc::{Sender, Receiver, channel};

use elasticsearch::{
    auth::Credentials, 
    Elasticsearch, 
    BulkOperation,
    BulkParts,
    http::{
        transport::{
            SingleNodeConnectionPool, 
            Transport, 
            TransportBuilder,
        },
        Url,
    }
};

use elasticsearch::cert::CertificateValidation;

#[derive(Deserialize, Default, Clone)]
pub enum CertValidation {
    #[default]
    Default,
    None
}

#[derive(Deserialize, Default)]
pub struct Elastic {
    url: Option<String>,
    username: Option<String>,
    password: Option<String>,
    cloud_id: Option<String>,
    index: String,
    cert_validation: Option<CertValidation>,
    #[serde(skip)]
    sender: Mutex<RefCell<Option<Sender<Request>>>>,
}

struct Request {
    message: Message,
    output: Sender<Result<(), Error>>,
}

impl Elastic {
    fn get_client(&self) -> Result<Elasticsearch, Error> {
        let cert_validation = match self.cert_validation.clone().unwrap_or(CertValidation::Default) {
            CertValidation::None => CertificateValidation::None,
            _ => CertificateValidation::Default,
        };

        if self.cloud_id.is_some() {
            let cloud_id = self.cloud_id.clone().unwrap();
            let username = self.username.clone().unwrap();
            let password = self.password.clone().unwrap();
            let credentials = Credentials::Basic(username, password);
            let transport = Transport::cloud(&cloud_id, credentials).map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
            Ok(Elasticsearch::new(transport))
        } else if self.username.is_some() {
            let url = self.url.clone().unwrap();
            let es_url = Url::parse(&url).map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
            let connection_pool = SingleNodeConnectionPool::new(es_url);
            let username = self.username.clone().unwrap();
            let password = self.password.clone().unwrap();
            let credentials = Credentials::Basic(username, password);
            let transport = TransportBuilder::new(connection_pool)
                .auth(credentials)
                .cert_validation(cert_validation)
                .build()
                .map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
            Ok(Elasticsearch::new(transport))
        } else if self.url.is_some() {
            let url = self.url.clone().unwrap();
            let es_url = Url::parse(&url).map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
            let connection_pool = SingleNodeConnectionPool::new(es_url);
            let transport = TransportBuilder::new(connection_pool)
                .cert_validation(cert_validation)
                .build()
                .map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
            Ok(Elasticsearch::new(transport))
        } else {
            Err(Error::ConfigFailedValidation("unable to determine connection type".into()))
        }
    }
}

async fn elasticsearch_handler(es_client: Elasticsearch, index: String, mut requests: Receiver<Request>) -> Result<(), Error> {
    while let Some(req) = requests.recv().await {
        let v: serde_json::Value = serde_json::from_slice(&req.message.bytes)?;

        let body: Vec<BulkOperation<_>> = vec![
            BulkOperation::index(v).into()
        ];

        let response = match es_client
            .bulk(BulkParts::Index(&index))
            .body(body)
            .send()
            .await {
                Ok(i) => i,
                Err(e) => {
                    let _ = req.output.send(Err(Error::OutputError(format!("{}", e)))).await;
                    continue
                },
            };

        let json: serde_json::Value = match response.json()
            .await {
                Ok(i) => i,
                Err(e) => {
                    let _ = req.output.send(Err(Error::OutputError(format!("{}", e)))).await;
                    continue
                }
            };

        match json["errors"].as_bool() {
            Some(_e) => {
                match json["items"].as_array() {
                    Some(arr) => {
                        let failed: Vec<String> = arr.iter()
                            .filter(|v| !v["error"].is_null())
                            .map(|v| format!("{}", v["error"]))
                            .collect();

                        if failed.len() > 0 {
                            let _ = req.output.send(Err(Error::OutputError(format!("failed to insert record: {}", failed.join(","))))).await;
                            continue
                        }
                    },
                    None => {
                        let _ = req.output.send(Err(Error::OutputError("unable to deteremine result".into()))).await;
                        continue
                    },
                }
            },
            None => {
                let _ = {
                    req.output.send(Err(Error::OutputError("unable to deteremine result".into()))).await;
                    continue
                };
            },
        };

        req.output.send(Ok(())).await;
    }
    println!("exiting");

    Ok(())
}

#[async_trait]
impl Output for Elastic {
    async fn write(&self, message: Message) -> Result<(), Error> {
        let s = self.sender.lock().await;
        let mut sender = s.replace(None);

        match sender {
            Some(ref mut r) => {
                let (tx, mut rx) = channel(1);
                r.send(Request {
                    message,
                    output: tx,
                }).await.unwrap();
                
                rx.recv().await.unwrap()?;

            },
            None => {},
        }
        Ok(())
    }
}

impl Closer for Elastic {
    fn close(&self) -> Result<(), Error> {
        Ok(())
    }
}

#[async_trait]
impl Connect for Elastic {
    async fn connect(&self) -> Result<(), Error> {
        let c = self.get_client()?;
        let (sender, receiver) = channel(1);
        tokio::spawn(elasticsearch_handler(c, self.index.clone(), receiver));

        let i = self.sender.lock().await;
        let mut s = i.borrow_mut();
        
        *s = Some(sender);
        
        Ok(())
    }
}

fn create_elasticsearch(conf: &Value) -> Result<ExecutionType, Error> {
    let elastic: Elastic = serde_yaml::from_value(conf.clone())?;
    if elastic.username.is_none() && elastic.password.is_some() {
        return Err(Error::ConfigFailedValidation("password is set but username is not".into()))
    } else if elastic.username.is_some() && elastic.password.is_none() {
        return Err(Error::ConfigFailedValidation("username is set but password is not".into()))
    } else if elastic.cloud_id.is_some() && (elastic.username.is_none() || elastic.password.is_none()) {
        return Err(Error::ConfigFailedValidation("cloud_id is set but username and/or password are not".into()))
    } else if elastic.cloud_id.is_none() && elastic.url.is_none() {
        return Err(Error::ConfigFailedValidation("cloud_id or url is required".into()))
    }

    Ok(ExecutionType::Output(Arc::new(elastic)))
}

// #[cfg_attr(feature = "elasticsearch", fiddler_registration_func)]
pub fn register_elasticsearch() -> Result<(), Error> {
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
  - index";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("elasticsearch".into(), ItemType::Output, conf_spec, create_elasticsearch)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_elasticsearch().unwrap()
    }
}