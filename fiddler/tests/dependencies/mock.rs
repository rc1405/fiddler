use async_trait::async_trait;
use fiddler::config::register_plugin;
use fiddler::config::ItemType;
use fiddler::config::{ConfigSpec, ExecutionType};
use fiddler::CallbackChan;
use fiddler::Message;
use fiddler::{Closer, Error, Input};
use fiddler_macros::fiddler_registration_func;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

#[derive(Serialize, Deserialize)]
pub struct MockInputConf {
    input: Vec<String>,
}

pub struct MockInput {
    input: Vec<String>,
}

#[async_trait]
impl Input for MockInput {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        match self.input.pop() {
            Some(i) => Ok((
                Message {
                    bytes: i.as_bytes().into(),
                    ..Default::default()
                },
                None,
            )),
            None => Err(Error::EndOfInput),
        }
    }
}

#[async_trait]
impl Closer for MockInput {}

#[fiddler_registration_func]
fn create_mock_input(conf: Value) -> Result<ExecutionType, Error> {
    let mut g: MockInputConf = serde_yaml::from_value(conf.clone())?;
    g.input = g.input.iter().rev().cloned().collect();

    Ok(ExecutionType::Input(Box::new(MockInput { input: g.input })))
}

pub fn register_mock_input() -> Result<(), Error> {
    let config = "type: object
properties:
  input: 
    type: array
    items: 
        type: string";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "mock_input".into(),
        ItemType::Input,
        conf_spec,
        create_mock_input,
    )
}
