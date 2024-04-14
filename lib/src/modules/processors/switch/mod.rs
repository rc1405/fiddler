use crate::{Error, Processor, Closer};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::{Item, ItemType, parse_configuration_item};
use crate::Message;
use crate::MessageBatch;
use serde_yaml::Value;
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use std::sync::Arc;

mod check;

pub struct Switch {
    steps: Vec<Arc<Box<dyn Processor + Send + Sync>>>,
}

#[async_trait]
impl Processor for Switch {
    async fn process(&self, message: Message) -> Result<MessageBatch, Error> {
        'steps: for p in &self.steps {
            println!("Starting new step");
            match p.process(message.clone()).await {
                Ok(m) => return Ok(m),
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
        Ok(vec![message])
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

fn create_switch(conf: &Value) -> Result<ExecutionType, Error> {
    let c: Vec<Item> = serde_yaml::from_value(conf.clone())?;
    let mut steps = Vec::new();
    for p in c {
        let ri = parse_configuration_item(ItemType::Processor, &p.extra)?;
        if let ExecutionType::Processor(rp) = ri.execution_type {
            steps.push(rp);
        };        
    };

    let s = Switch{
        steps,
    };

    Ok(ExecutionType::Processor(Arc::new(Box::new(s))))
}

#[fiddler_registration_func]
pub fn register_switch() -> Result<(), Error> {
    let config = "type: array";

    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("switch".into(), ItemType::Processor, conf_spec, create_switch)?;
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