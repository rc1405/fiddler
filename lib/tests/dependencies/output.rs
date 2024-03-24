use std::cell::RefCell;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use regex::Regex;

use fiddler::{Error, Output, Closer, Connect};
use fiddler::config::{ConfigSpec, ExecutionType};
use fiddler::config::register_plugin;
use fiddler::config::ItemType;
use fiddler::Message;
use fiddler_macros::fiddler_registration_func;


#[derive(Deserialize, Serialize)]
struct ValidateSpec {
    expected: isize,
    prefix: String,
}

pub struct Validate {
    expected: Mutex<RefCell<isize>>,
    total: isize,
    prefix: String,
}

#[async_trait]
impl Output for Validate {
    async fn write(self: &Self, message: Message) -> Result<(), Error> {
        let prefix = &self.prefix;
        println!("received prefix {prefix}");
        let pattern = format!(r#"{prefix}: Hello World (\d+)|{prefix}: \{{\"Hello World\": (\d+)\}}|^\{{\"Hello World\": (\d+), \"Python\": \"rocks\"\}}"#);
        println!("Have pattern: {pattern}");
        let re = Regex::new(&pattern).unwrap();
        let msg = String::from_utf8(message.bytes).unwrap();
        println!("{}", msg);

        match self.expected.lock() {
            Ok(c) => {
                let mut count = c.borrow_mut();
                *count-=1;

                if *count < 0 {
                    panic!("Called an extra time");
                };

                let call = re.captures(&msg).unwrap().get(1).map_or(0, |m| m.as_str().parse::<isize>().unwrap());
                if call > self.total {
                    panic!("Received an extra event")
                };     
                
        
                Ok(())
            },
            Err(_) => return Err(Error::ExecutionError(format!("Unable to get inner lock")))
        }  
    }
}

impl Closer for Validate {
    fn close(self: &Self) -> Result<(), Error> {
        let c = *self.expected.lock().unwrap().borrow_mut();
        if c != 0 {
            panic!("missing calls: {}", c);
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
    return Ok(ExecutionType::Output(Box::new(Validate{
        expected: Mutex::new(RefCell::new(g.expected.clone())),
        total: g.expected,
        prefix: g.prefix,
    })))
}

#[fiddler_registration_func]
pub fn register_validate() -> Result<(), Error> {
    let config = "type: object
properties:
  expected:
    type: number
  prefix:
    type: string";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("validate".into(), ItemType::Output, conf_spec, create_validator)
}