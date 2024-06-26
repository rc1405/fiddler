use async_trait::async_trait;
use fiddler::config::register_plugin;
use fiddler::config::ItemType;
use fiddler::config::{ConfigSpec, ExecutionType};
use fiddler::Message;
use fiddler::{new_callback_chan, CallbackChan};
use fiddler::{Closer, Connect, Error, Input};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

#[derive(Serialize, Deserialize)]
pub struct MockInputConf {
    input: Vec<String>,
}

pub struct MockInput {
    input: Mutex<RefCell<Vec<String>>>,
}

#[async_trait]
impl Input for MockInput {
    async fn read(self: &Self) -> Result<(Message, CallbackChan), Error> {
        match self.input.lock() {
            Ok(c) => {
                let mut input = c.borrow_mut();

                let (tx, rx) = new_callback_chan();
                tokio::spawn(async move { rx.await });

                match input.pop() {
                    Some(i) => Ok((
                        Message {
                            bytes: i.as_bytes().into(),
                            ..Default::default()
                        },
                        tx,
                    )),
                    None => Err(Error::EndOfInput),
                }
            }
            Err(_) => return Err(Error::ExecutionError(format!("Unable to get inner lock"))),
        }
    }
}

impl Closer for MockInput {
    fn close(self: &Self) -> Result<(), Error> {
        Ok(())
    }
}

impl Connect for MockInput {
    fn connect(self: &Self) -> Result<(), Error> {
        Ok(())
    }
}

fn create_mock_input(conf: &Value) -> Result<ExecutionType, Error> {
    let mut g: MockInputConf = serde_yaml::from_value(conf.clone())?;
    g.input = g.input.iter().rev().map(|i| i.clone()).collect();

    return Ok(ExecutionType::Input(Arc::new(Box::new(MockInput {
        input: Mutex::new(RefCell::new(g.input)),
    }))));
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
