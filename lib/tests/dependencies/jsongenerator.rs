use async_trait::async_trait;
use fiddler::config::register_plugin;
use fiddler::config::ItemType;
use fiddler::config::{ConfigSpec, ExecutionType};
use fiddler::Callback;
use fiddler::Message;
use fiddler::{Closer, Connect, Error, Input};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::cell::RefCell;
use std::sync::Mutex;

#[derive(Serialize, Deserialize)]
pub struct JsonGenerator {
    count: Mutex<RefCell<isize>>,
}

#[async_trait]
impl Input for JsonGenerator {
    async fn read(self: &Self) -> Result<(Message, Callback), Error> {
        match self.count.lock() {
            Ok(c) => {
                let mut count = c.borrow_mut();

                if *count <= 0 {
                    return Err(Error::EndOfInput);
                };

                *count -= 1;

                Ok((
                    Message {
                        bytes: format!("{{\"Hello World\": {}}}", count).as_bytes().into(),
                    },
                    handle_message,
                ))
            }
            Err(_) => return Err(Error::ExecutionError(format!("Unable to get inner lock"))),
        }
    }
}

fn handle_message(_msg: Message) -> Result<(), Error> {
    Ok(())
}

impl Closer for JsonGenerator {
    fn close(self: &Self) -> Result<(), Error> {
        Ok(())
    }
}

impl Connect for JsonGenerator {
    fn connect(self: &Self) -> Result<(), Error> {
        Ok(())
    }
}

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
