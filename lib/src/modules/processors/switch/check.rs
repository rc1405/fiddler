use std::convert::Into;
use crate::{Error, Processor, Closer};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::{Item, ItemType, parse_configuration_item};
use crate::Message;
use crate::MessageBatch;
use serde_yaml::Value;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use std::mem;

#[derive(Deserialize, Serialize)]
struct CheckConfig {
    label: Option<String>,
    condition: String,
    processors: Vec<Item>,
}

pub struct Check {
    _label: Option<String>,
    condition: String,
    processors: Vec<Box<dyn Processor + Send + Sync>>,
}

fn perform_check(condition: &str, json_str: String) -> Result<(), Error> {
    let mut runtime = jmespath::Runtime::new();
    runtime.register_builtin_functions();
    let expr = runtime.compile(condition).map_err(|e| Error::ProcessingError(format!("{}", e)))?;
    // convert to string if this is nothing.
    let data = jmespath::Variable::from_json(&json_str).map_err(|e| Error::ProcessingError(e.to_string()))?;

    let result = expr.search(data).map_err(|e| Error::ProcessingError(format!("{}", e)))?;
    if !result.as_boolean().unwrap_or(false) {
        return Err(Error::ConditionalCheckfailed)
    };
    Ok(())
}

#[async_trait]
impl Processor for Check {
    async fn process(&self, message: Message) -> Result<MessageBatch, Error> {
        let mut messages = vec![message.clone()];
        let json_str = String::from_utf8(message.bytes).map_err(|e| Error::ProcessingError(format!("{}", e)))?;

        perform_check(&self.condition, json_str)?;
        
        for p in &self.processors {
            let mut new_messages = Vec::new();
            while let Some(m) = messages.pop() {
                new_messages = messages.clone();
                let m = p.process(m.clone()).await?;
                new_messages.extend(m);
            };
            println!("new messages: {:?}", new_messages);
            let _ = mem::replace(&mut messages, new_messages);
        };
        Ok(messages)
    }
}

impl Closer for Check {
    fn close(&self) -> Result<(), Error> {
        for p in &self.processors {
            p.close()?;
        };
        Ok(())
    }
}

fn create_check(conf: &Value) -> Result<ExecutionType, Error> {
    let c: CheckConfig = serde_yaml::from_value(conf.clone())?;
    let _ = jmespath::compile(&c.condition).map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;

    let mut steps = Vec::new();
    for p in c.processors {
        let ri = parse_configuration_item(ItemType::Processor, &p.extra)?;
        if let ExecutionType::Processor(rp) = ri.execution_type {
            steps.push(rp);
        };        
    };

    let s = Check{
        _label: c.label,
        condition: c.condition,
        processors: steps,
    };

    Ok(ExecutionType::Processor(Box::new(s)))
}

pub fn register_check() -> Result<(), Error> {
    let config = "type: object
properties:
  label:
    type: string
  condition:
    type: string
  processors:
    type: array";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("check".into(), ItemType::Processor, conf_spec, create_check)
}