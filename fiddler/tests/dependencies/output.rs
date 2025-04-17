use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use fiddler::config::register_plugin;
use fiddler::config::ItemType;
use fiddler::config::{ConfigSpec, ExecutionType};
use fiddler::Message;
use fiddler::{Closer, Error, Output};

#[derive(Deserialize, Serialize)]
struct ValidateSpec {
    expected: Vec<String>,
}

pub struct Validate {
    expected: Vec<String>,
    count: usize,
}

#[async_trait]
impl Output for Validate {
    async fn write(&mut self, message: Message) -> Result<(), Error> {
        let msg_str = String::from_utf8(message.bytes).unwrap();

        if self.count > self.expected.len() - 1 {
            panic!("Received an extra event")
        };
        if self.expected[self.count] != msg_str {
            panic!(
                "Received unexpected message.  \n\tExpected {}, \n\treceived {}",
                self.expected[self.count], msg_str
            );
        };

        self.count += 1;

        return Ok(());
    }
}

#[async_trait]
impl Closer for Validate {
    async fn close(&mut self) -> Result<(), Error> {
        if self.count != self.expected.len() {
            panic!(
                "received {} calls: expected {}",
                self.count,
                self.expected.len()
            );
        };
        Ok(())
    }
}

fn create_validator(conf: &Value) -> Result<ExecutionType, Error> {
    let g: ValidateSpec = serde_yaml::from_value(conf.clone())?;
    Ok(ExecutionType::Output(Box::new(Validate {
        expected: g.expected.clone(),
        count: 0,
    })))
}

pub fn register_validate() -> Result<(), Error> {
    let config = "type: object
properties:
  expected:
    type: array
    items:
      type: string";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "validate".into(),
        ItemType::Output,
        conf_spec,
        create_validator,
    )
}
