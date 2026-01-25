use crate::config::register_plugin;
use crate::config::{parse_configuration_item, Item, ItemType};
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::MessageBatch;
use crate::{Closer, Error, Processor};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use serde::Deserialize;
use serde_yaml::Value;
use tracing::debug;

#[derive(Deserialize)]
struct TryConfig {
    processor: Value,
    catch: Option<Vec<Item>>,
}
pub struct Try {
    processor: Box<dyn Processor + Send + Sync>,
    catch: Vec<Box<dyn Processor + Send + Sync>>,
}

#[async_trait]
impl Processor for Try {
    async fn process(&self, message: Message) -> Result<MessageBatch, Error> {
        match self.processor.process(message.clone()).await {
            Ok(m) => Ok(m),
            Err(e) => {
                debug!("caught error {e}");
                let mut messages = vec![message];
                for p in &self.catch {
                    let mut new_messages = Vec::new();
                    for m in messages.drain(..) {
                        let processed = p.process(m).await?;
                        new_messages.extend(processed);
                    }
                    messages = new_messages;
                }
                Ok(messages)
            }
        }
    }
}

#[async_trait]
impl Closer for Try {
    async fn close(&mut self) -> Result<(), Error> {
        self.processor.close().await?;
        for p in &mut self.catch {
            p.close().await?;
        }
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_try(conf: Value) -> Result<ExecutionType, Error> {
    let try_conf: TryConfig = serde_yaml::from_value(conf.clone())?;
    let proc: Item = serde_yaml::from_value(try_conf.processor)?;

    let proc_registered_item = parse_configuration_item(ItemType::Processor, &proc.extra).await?;
    let p = match ((proc_registered_item.creator)(proc_registered_item.config)).await? {
        ExecutionType::Processor(rp) => rp,
        _ => {
            return Err(Error::ConfigFailedValidation(
                "invalid execution type provided".into(),
            ))
        }
    };

    let catch: Vec<Box<dyn Processor + Send + Sync>> = match try_conf.catch {
        Some(processors) => {
            let mut steps = Vec::new();
            for p in processors {
                let ri = parse_configuration_item(ItemType::Processor, &p.extra).await?;
                let proc = ((ri.creator)(ri.config.clone())).await?;
                if let ExecutionType::Processor(rp) = proc {
                    steps.push(rp);
                };
            }
            steps
        }
        None => Vec::new(),
    };

    Ok(ExecutionType::Processor(Box::new(Try {
        processor: p,
        catch,
    })))
}

pub(super) fn register_try() -> Result<(), Error> {
    let config = "type: object
properties:
  processor: 
    type: object
  catch:
    type: array
required:
  - processor";

    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("try".into(), ItemType::Processor, conf_spec, create_try)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_try().unwrap()
    }
}
