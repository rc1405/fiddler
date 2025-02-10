use crate::config::register_plugin;
use crate::config::{parse_configuration_item, Item, ItemType};
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::MessageBatch;
use crate::{Closer, Error, Processor};
use async_trait::async_trait;
use serde_yaml::Value;
mod check;

pub struct Switch {
    steps: Vec<Box<dyn Processor + Send + Sync>>,
}

#[async_trait]
impl Processor for Switch {
    async fn process(&self, message: Message) -> Result<MessageBatch, Error> {
        'steps: for p in &self.steps {
            match p.process(message.clone()).await {
                Ok(m) => return Ok(m),
                Err(e) => match e {
                    Error::ConditionalCheckfailed => continue 'steps,
                    _ => return Err(e),
                },
            };
        }
        Ok(vec![message])
    }
}

#[async_trait]
impl Closer for Switch {
    async fn close(&mut self) -> Result<(), Error> {
        for p in &mut self.steps {
            p.close().await?;
        }
        Ok(())
    }
}

fn create_switch(conf: &Value) -> Result<ExecutionType, Error> {
    let c: Vec<Item> = serde_yaml::from_value(conf.clone())?;
    let mut steps = Vec::new();
    for p in c {
        let ri = parse_configuration_item(ItemType::Processor, &p.extra)?;
        let proc = ((ri.creator)(&ri.config))?;
        if let ExecutionType::Processor(rp) = proc {
            steps.push(rp);
        };
    }

    let s = Switch { steps };

    Ok(ExecutionType::Processor(Box::new(s)))
}

pub fn register_switch() -> Result<(), Error> {
    let config = "type: array";

    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "switch".into(),
        ItemType::Processor,
        conf_spec,
        create_switch,
    )?;
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
