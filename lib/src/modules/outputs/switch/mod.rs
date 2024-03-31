use crate::{Error, Output, Connect, Closer};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::{Item, ItemType, parse_configuration_item};
use crate::Message;
use serde_yaml::Value;
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;

mod check;

pub struct Switch {
    steps: Vec<Box<dyn Output + Send + Sync>>,
}

#[async_trait]
impl Output for Switch {
    async fn write(&self, message: Message) -> Result<(), Error> {
        'steps: for p in &self.steps {
            println!("Starting new step");
            match p.write(message.clone()).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    match e {
                        Error::ConditionalCheckfailed => {
                            println!("Errored: {}", e);
                            continue 'steps
                        },
                        _ => {
                            return Err(e)
                        },
                    }
                }
            };
        };
        Ok(())
    }
}

impl Closer for Switch {
    fn close(&self) -> Result<(), Error> {
        for p in &self.steps {
            p.close()?;
        };
        Ok(())
    }
}

impl Connect for Switch {
    fn connect(&self) -> Result<(), Error> {
        Ok(())
    }
}

fn create_switch(conf: &Value) -> Result<ExecutionType, Error> {
    let c: Vec<Item> = serde_yaml::from_value(conf.clone())?;
    let mut steps = Vec::new();
    for p in c {
        let ri = parse_configuration_item(ItemType::Output, &p.extra)?;
        if let ExecutionType::Output(rp) = ri.execution_type {
            steps.push(rp);
        };        
    };

    let s = Switch{
        steps,
    };

    Ok(ExecutionType::Output(Box::new(s)))
}

#[fiddler_registration_func]
pub fn register_switch() -> Result<(), Error> {
    let config = "type: array";

    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("switch".into(), ItemType::Output, conf_spec, create_switch)?;
    check::register_check()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_switch().unwrap()
    }
}