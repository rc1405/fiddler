use crate::config::register_plugin;
use crate::config::{parse_configuration_item, Item, ItemType};
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::{Closer, Error, Output};
use async_trait::async_trait;
use serde_yaml::Value;
mod check;

pub struct Switch {
    steps: Vec<Box<dyn Output + Send + Sync>>,
}

#[async_trait]
impl Output for Switch {
    async fn write(&mut self, message: Message) -> Result<(), Error> {
        'steps: for p in &mut self.steps {
            match p.write(message.clone()).await {
                Ok(_) => return Ok(()),
                Err(e) => match e {
                    Error::ConditionalCheckfailed => continue 'steps,
                    _ => return Err(e),
                },
            };
        }
        Ok(())
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
        let ri = parse_configuration_item(ItemType::Output, &p.extra)?;
        let step = ((ri.creator)(&ri.config))?;
        if let ExecutionType::Output(rp) = step {
            steps.push(rp);
        };
    }

    let s = Switch { steps };

    Ok(ExecutionType::Output(Box::new(s)))
}

pub (super) fn register_switch() -> Result<(), Error> {
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
