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
pub struct Generator {
    count: isize,
}

#[async_trait]
impl Input for Generator {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        if self.count <= 0 {
            return Err(Error::EndOfInput);
        }

        self.count -= 1;
        Ok((
            Message {
                bytes: format!("Hello World {}", self.count).as_bytes().into(),
                ..Default::default()
            },
            None,
        ))
    }
}

#[async_trait]
impl Closer for Generator {}

#[fiddler_registration_func]
fn create_generator(conf: Value) -> Result<ExecutionType, Error> {
    let g: Generator = serde_yaml::from_value(conf.clone())?;
    Ok(ExecutionType::Input(Box::new(g)))
}

pub fn register_generator() -> Result<(), Error> {
    let config = "type: object
properties:
  count: 
    type: number";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "generator".into(),
        ItemType::Input,
        conf_spec,
        create_generator,
    )
}
