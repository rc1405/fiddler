use crate::{Error, Output, Closer, Connect};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use serde_yaml::Value;
use async_trait::async_trait;
use serde::Deserialize;
use fiddler_macros::fiddler_registration_func;

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

#[derive(Deserialize, Default)]
pub struct Elastic {
    url: Option<String>,
    username: Option<String>,
    password: Option<String>,
    cloud_id: Option<String>,
    index: String,
}

impl Elastic {
    fn get_client(&self) -> Result<Elasticsearch, Error> {
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
                .build()
                .map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
            Ok(Elasticsearch::new(transport))
        } else if self.url.is_some() {
            let url = self.url.clone().unwrap();
            let transport = Transport::single_node(&url).map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;
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
            Some(e) => {
                match json["items"].as_array() {
                    Some(arr) => {
                        let failed = arr.iter()
                            .filter(|v| !v["error"].is_null())
                            .collect();

                        if failed.len() > 0 {
                            return Error::OutputError("failed to insert record: {}", failed.join(","))
                        }
                    },
                    None => return Error::OutputError("unable to deteremine result".into()),
                }
            },
            None => return Error::OutputError("unable to deteremine result".into()),
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

    Ok(ExecutionType::Output(Box::new(elastic)))
}

#[cfg_attr(feature = "elasticsearch", fiddler_registration_func)]
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