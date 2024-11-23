use crate::{Error, Output, Closer, Connect};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use serde_yaml::Value;
use async_trait::async_trait;
use serde::Deserialize;
use fiddler_macros::fiddler_registration_func;
use std::sync::Arc;

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

#[async_trait]
impl Output for Elastic {
    async fn write(&self, message: Message) -> Result<(), Error> {
        let es_client = self.get_client()?;

        let v: serde_json::Value = serde_json::from_slice(&message.bytes)?;

        let body: Vec<BulkOperation<_>> = vec![
            BulkOperation::index(v).into()
        ];

        let response = es_client
            .bulk(BulkParts::Index(&self.index))
            .body(body)
            .send()
            .await
            .map_err(|e| Error::OutputError(format!("{}", e)))?;

        let json: serde_json::Value = response.json()
            .await
            .map_err(|e| Error::OutputError(format!("{}", e)))?;

        match json["errors"].as_bool() {
            Some(_e) => {
                match json["items"].as_array() {
                    Some(arr) => {
                        let failed: Vec<String> = arr.iter()
                            .filter(|v| !v["error"].is_null())
                            .map(|v| format!("{}", v["error"]))
                            .collect();

                        if failed.len() > 0 {
                            return Err(Error::OutputError(format!("failed to insert record: {}", failed.join(","))))
                        }
                    },
                    None => return Err(Error::OutputError("unable to deteremine result".into())),
                }
            },
            None => return Err(Error::OutputError("unable to deteremine result".into())),
        }

        Ok(())
    }
}

impl Closer for Elastic {
    fn close(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl Connect for Elastic {
    fn connect(&self) -> Result<(), Error> {
        let _ = self.get_client()?;
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

    Ok(ExecutionType::Output(Arc::new(Box::new(elastic))))
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