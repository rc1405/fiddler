use std::cell::RefCell;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use fiddler::config::register_plugin;
use fiddler::config::ItemType;
use fiddler::config::{ConfigSpec, ExecutionType};
use fiddler::Message;
use fiddler::{Closer, Connect, Error, Output};

#[derive(Deserialize, Serialize)]
struct AssertSpec {
    expected: Vec<String>,
}

pub struct Assert {
    expected: Vec<String>,
    count: Mutex<RefCell<usize>>,
    errors: Mutex<RefCell<Vec<String>>>,
}

#[async_trait]
impl Output for Assert {
    async fn write(&self, message: Message) -> Result<(), Error> {
        let msg_str = String::from_utf8(message.bytes).unwrap();

        match self.count.lock() {
            Ok(c) => {
                let mut count = c.borrow_mut();
                match self.errors.lock() {
                    Ok(err) => {
                        let mut errors = err.borrow_mut();

                        if *count > self.expected.len() - 1 {
                            errors.push(format!("Received unexpected message: {}", msg_str));
                            *count += 1;
                            return Ok(());
                        };

                        if self.expected[*count] != msg_str {
                            errors.push(format!(
                                "Received unexpected message.  \n  Expected: {} \n  Actual:   {}",
                                self.expected[*count], msg_str
                            ));
                        };

                        *count += 1;

                        return Ok(());
                    }
                    Err(_) => return Err(Error::ExecutionError("Unable to get inner lock".into())),
                };
            }
            Err(_) => return Err(Error::ExecutionError("Unable to get inner lock".into())),
        }
    }
}

impl Closer for Assert {
    fn close(&self) -> Result<(), Error> {
        let c = *self.count.lock().unwrap().borrow_mut();
        let errors = self.errors.lock().unwrap();
        let mut err = errors.borrow_mut();

        if c != self.expected.len() {
            err.push(format!(
                "Received {} calls: expected {}",
                c,
                self.expected.len()
            ));
        };

        if err.len() > 0 {
            return Err(Error::ExecutionError(err.join("\n")));
        };

        Ok(())
    }
}

impl Connect for Assert {
    fn connect(&self) -> Result<(), Error> {
        Ok(())
    }
}

fn create_assert(conf: &Value) -> Result<ExecutionType, Error> {
    let g: AssertSpec = serde_yaml::from_value(conf.clone())?;
    Ok(ExecutionType::Output(Arc::new(Box::new(Assert {
        expected: g.expected.clone(),
        count: Mutex::new(RefCell::new(0)),
        errors: Mutex::new(RefCell::new(Vec::new())),
    }))))
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
