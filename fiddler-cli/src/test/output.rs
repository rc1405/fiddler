use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use fiddler::config::register_plugin;
use fiddler::config::ItemType;
use fiddler::config::{ConfigSpec, ExecutionType};
use fiddler::Message;
use fiddler::{Closer, Error, Output};
use fiddler_macros::fiddler_registration_func;

#[derive(Deserialize, Serialize)]
struct AssertSpec {
    expected: Vec<String>,
}

pub struct Assert {
    expected: Vec<String>,
    count: usize,
    errors: Vec<String>,
}

#[async_trait]
impl Output for Assert {
    async fn write(&mut self, message: Message) -> Result<(), Error> {
        let msg_str = String::from_utf8(message.bytes).unwrap();

        if self.count > self.expected.len() - 1 {
            self.errors
                .push(format!("Received unexpected message: {}", msg_str));
            self.count += 1;
            return Ok(());
        };

        if self.expected[self.count] != msg_str {
            self.errors.push(format!(
                "Received unexpected message.  \n  Expected: {} \n  Actual:   {}",
                self.expected[self.count], msg_str
            ));
        };

        self.count += 1;
        return Ok(());
    }
}

#[async_trait]
impl Closer for Assert {
    async fn close(&mut self) -> Result<(), Error> {
        if self.count != self.expected.len() {
            self.errors.push(format!(
                "Received {} calls: expected {}",
                self.count,
                self.expected.len()
            ));
        };

        if !self.errors.is_empty() {
            return Err(Error::ExecutionError(self.errors.join("\n")));
        };

        Ok(())
    }
}

#[fiddler_registration_func]
fn create_assert(conf: Value) -> Result<ExecutionType, Error> {
    let g: AssertSpec = serde_yaml::from_value(conf.clone())?;
    Ok(ExecutionType::Output(Box::new(Assert {
        expected: g.expected.clone(),
        count: 0,
        errors: Vec::new(),
    })))
}

pub fn register_assert() -> Result<(), Error> {
    let config = "type: object
properties:
  expected:
    type: array
    items:
      type: string";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("assert".into(), ItemType::Output, conf_spec, create_assert)
}
