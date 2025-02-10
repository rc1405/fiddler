use async_trait::async_trait;
use fiddler::config::register_plugin;
use fiddler::config::ItemType;
use fiddler::config::{ConfigSpec, ExecutionType};
use fiddler::Message;
use fiddler::{new_callback_chan, CallbackChan};
use fiddler::{Closer, Error, Input};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

#[derive(Serialize, Deserialize)]
pub struct JsonGenerator {
    count: isize,
}

#[async_trait]
impl Input for JsonGenerator {
    async fn read(&mut self) -> Result<(Message, CallbackChan), Error> {
        if self.count <= 0 {
            return Err(Error::EndOfInput);
        };

        self.count -= 1;

        let (tx, rx) = new_callback_chan();
        tokio::spawn(async move { rx.await });

        Ok((
            Message {
                bytes: format!("{{\"Hello World\": {}}}", self.count)
                    .as_bytes()
                    .into(),
                ..Default::default()
            },
            tx,
        ))
    }
}

#[async_trait]
impl Closer for JsonGenerator {}

fn create_json_generator(conf: &Value) -> Result<ExecutionType, Error> {
    let g: JsonGenerator = serde_yaml::from_value(conf.clone())?;
    return Ok(ExecutionType::Input(Box::new(g)));
}

pub fn register_json_generator() -> Result<(), Error> {
    let config = "type: object
properties:
  count: 
    type: number";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "json_generator".into(),
        ItemType::Input,
        conf_spec,
        create_json_generator,
    )
}
