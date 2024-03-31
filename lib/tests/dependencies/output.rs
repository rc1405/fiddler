use std::cell::RefCell;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use fiddler::config::register_plugin;
use fiddler::config::ItemType;
use fiddler::config::{ConfigSpec, ExecutionType};
use fiddler::Message;
use fiddler::{Closer, Connect, Error, Output};
use fiddler_macros::fiddler_registration_func;

#[derive(Deserialize, Serialize)]
struct ValidateSpec {
    expected: Vec<String>,
}

pub struct Validate {
    expected: Vec<String>,
    count: Mutex<RefCell<usize>>,
}

#[async_trait]
impl Output for Validate {
    async fn write(self: &Self, message: Message) -> Result<(), Error> {
        let msg_str = String::from_utf8(message.bytes).unwrap();

        match self.count.lock() {
            Ok(c) => {
                let mut count = c.borrow_mut();
                if *count > self.expected.len() - 1 {
                    panic!("Received an extra event")
                };
                if self.expected[*count] != msg_str {
                    panic!(
                        "Received unexpected message.  \n\tExpected {}, \n\treceived {}",
                        self.expected[*count], msg_str
                    );
                };

                *count += 1;

                return Ok(());
            }
            Err(_) => return Err(Error::ExecutionError(format!("Unable to get inner lock"))),
        }
    }
}

impl Closer for Validate {
    fn close(self: &Self) -> Result<(), Error> {
        let c = *self.count.lock().unwrap().borrow_mut();
        if c != self.expected.len() {
            panic!("received {} calls: expected {}", c, self.expected.len());
        };
        Ok(())
    }
}

impl Connect for Validate {
    fn connect(self: &Self) -> Result<(), Error> {
        Ok(())
    }
}

fn create_validator(conf: &Value) -> Result<ExecutionType, Error> {
    let g: ValidateSpec = serde_yaml::from_value(conf.clone())?;
    return Ok(ExecutionType::Output(Box::new(Validate {
        expected: g.expected.clone(),
        count: Mutex::new(RefCell::new(0)),
    })));
}

#[fiddler_registration_func]
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
