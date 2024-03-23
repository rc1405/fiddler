use std::collections::HashMap;
use std::sync::Mutex;
use std::fmt;

use jsonschema::{Draft, JSONSchema};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use log::{debug, error, info, warn, trace};

use crate::{InputBatch, OutputBatch};
use super::{Input, Output, Processor, Error};

mod registration;
pub use registration::register_plugin;


type Callback = fn(&Value) -> Result<ExecutionType, Error>;

#[derive(PartialEq, Eq, Hash)]
pub enum ItemType {
    Input,
    InputBatch,
    Output,
    OutputBatch,
    Processor,
}

impl fmt::Display for ItemType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let msg = match self {
            ItemType::Input => "input",
            ItemType::InputBatch => "input_batch",
            ItemType::Output => "output",
            ItemType::OutputBatch => "output_batch",
            ItemType::Processor => "processors",
        };
        write!(f, "{}", msg)
    }
}

pub enum ExecutionType {
    Input(Box<dyn Input + Send + Sync>),
    InputBatch(Box<dyn InputBatch + Send + Sync>),
    Output(Box<dyn Output + Send + Sync>),
    OutputBatch(Box<dyn OutputBatch + Send + Sync>),
    Processor(Box<dyn Processor + Send + Sync>),
}

static ENV: Lazy<Mutex<HashMap<ItemType, HashMap<String, RegisteredItem>>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(ItemType::Input, HashMap::new());
    m.insert(ItemType::InputBatch, HashMap::new());
    m.insert(ItemType::Output, HashMap::new());
    m.insert(ItemType::OutputBatch, HashMap::new());
    m.insert(ItemType::Processor, HashMap::new());
    Mutex::new(m)
});

#[derive(Clone)]
pub struct RegisteredItem {
    pub creator: Callback,
    pub format: ConfigSpec,
}

pub struct ParsedRegisteredItem {
    pub item_type: ItemType,
    pub execution_type: ExecutionType,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Pipeline {
    pub label: Option<String>,
    pub processors: Vec<Item>,
}

pub struct ParsedPipeline {
    pub label: Option<String>,
    pub processors: Vec<ParsedRegisteredItem>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Item {
    pub label: Option<String>,

    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}


#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub input: Item,
    pub pipeline: Pipeline,
    pub output: Item,
}

impl Config {
    fn check_item(&self, itype: ItemType, map: &HashMap<String, Value>) -> Result<ParsedRegisteredItem, Error> {
        let keys: Vec<String> = map.keys().into_iter().map(|k| k.clone()).collect();
        let first_key = keys.first().ok_or(
            Error::ConfigFailedValidation(format!("unable to determine {} key", itype))
        )?;
        let item = self.get_item(&itype, first_key)?;

        let content = map.get(first_key).ok_or(
            Error::ConfigFailedValidation(format!("unable to validate {} key {}", itype, first_key))
        )?;
        let content_str = serde_yaml::to_string(content)?;
        item.format.validate(&content_str)?;
        match itype {
            ItemType::Input => {
                let creator = (item.creator)(content)?;
                match &creator {
                    ExecutionType::Input(_) => {},
                    _ => return Err(Error::ConfigFailedValidation("invalid type returned for input".into())),
                };

                Ok(ParsedRegisteredItem{
                    execution_type: creator,
                    item_type: ItemType::Input,
                })
            },
            ItemType::InputBatch => Err(Error::NotYetImplemented),
            ItemType::Output => {
                let creator = (item.creator)(content)?;
                match &creator {
                    ExecutionType::Output(_) => {},
                    _ => return Err(Error::ConfigFailedValidation("invalid type returned for output".into())),
                };

                Ok(ParsedRegisteredItem{
                    execution_type: creator,
                    item_type: ItemType::Output,
                })
            },
            ItemType::OutputBatch => Err(Error::NotYetImplemented),
            ItemType::Processor => {
                let creator = (item.creator)(content)?;
                match &creator {
                    ExecutionType::Processor(_) => {},
                    _ => return Err(Error::ConfigFailedValidation("invalid type returned for processor".into())),
                };

                Ok(ParsedRegisteredItem{
                    execution_type: creator,
                    item_type: ItemType::Processor,
                })
            }
        }
    }

    pub fn validate(self) -> Result<ParsedConfig, Error> {
        if self.input.extra.len() > 1 {
            return Err(Error::Validation("input must only contain one entry".into()))
        };

        if self.output.extra.len() > 1 {
            return Err(Error::Validation("output must only contain one entry".into()))
        };

        if self.pipeline.processors.len() == 0 {
            return Err(Error::Validation("pipeline must contain at least one processor".into()))
        };
        
        let input = self.check_item(ItemType::Input, &self.input.extra)?;

        let output = self.check_item(ItemType::Output, &self.output.extra)?;

        let mut processors = Vec::new();

        for p in &self.pipeline.processors {
            let proc = self.check_item(ItemType::Processor, &p.extra)?;
            processors.push(proc);
        };

        let parsed_pipeline = ParsedPipeline{
            label: self.pipeline.label.clone(),
            processors,
        };

        Ok(ParsedConfig{
            _config: self,
            input,
            pipeline: parsed_pipeline,
            output,
        })
    }

    fn get_item(&self, itype: &ItemType, key: &String) -> Result<RegisteredItem, Error> {
        match ENV.lock() {
            Ok(lock) => {
                match lock.get(itype) {
                    Some(i) => {
                        if let Some(item) = i.get(key) {
                            return Ok(item.clone())
                        }
                    },
                    None => return Err(Error::UnableToSecureLock),
                };
            },
            Err(_) => return Err(Error::UnableToSecureLock),
        };
        return Err(Error::ConfigurationItemNotFound(key.clone()))
    }
}

pub struct ParsedConfig {
    _config: Config,
    pub input: ParsedRegisteredItem,
    pub pipeline: ParsedPipeline,
    pub output: ParsedRegisteredItem,
}

#[derive(Debug)]
pub struct ConfigSpec {
    raw_schema: String,
    schema: JSONSchema,
}

impl Clone for ConfigSpec {
    fn clone(&self) -> Self {
        ConfigSpec::from_schema(&self.raw_schema).unwrap()
    }
}

impl ConfigSpec {
    pub fn from_schema<'a>(conf: &str) -> Result<Self, Error> {
        let v: Value = serde_yaml::from_str(conf)?;
        let intermediate = serde_json::to_string(&v)?;
        let f: serde_json::Value = serde_json::from_str(&intermediate)?;

        let schema: JSONSchema = match JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(&f) {
                Ok(js) => js,
                Err(e) => return Err(Error::InvalidValidationSchema(format!("{}", e)))
            };

        Ok(ConfigSpec{
            raw_schema: conf.into(),
            schema,
        })
    }

    pub fn validate(self: &Self, content: &str) -> Result<(), Error> {
        let v: Value = serde_yaml::from_str(content)?;
        let intermediate = serde_json::to_string(&v)?;
        let f: serde_json::Value = serde_json::from_str(&intermediate)?;
        let result = self.schema.validate(&f);
        if let Err(errors) = result {
            let errs: Vec<String> = errors.into_iter().map(|i| format!("{}", i)).collect();
            return Err(Error::ConfigFailedValidation(errs.join(" ")))
        };
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn load_configuration() {
        let input = "input:
    stdin:
        scanner:
            lines: {}
pipeline:
    processors:
        - label: my_cool_mapping
          mapping: |
            root.message = this
            root.meta.link_count = this.links.length()
output:
    label: my_s3_output    
    aws_s3:
        bucket: TODO
        path: TODO";

        let _v: Config = serde_yaml::from_str(input).unwrap();
    }

    #[test]
    fn validate_configuration_item() {
        let input = "scanner:
    lines: true";

        let schema = "properties:
    scanner: 
        type: object
        properties:
            lines:
                type: boolean";

        let conf = ConfigSpec::from_schema(schema).unwrap();
        conf.validate(input).unwrap();
    }

    #[test]
    fn expect_schema_failure() {
        let input = "scanner:
    lines: true";

        let schema = "properties:
    scanner: 
        type: object
        properties:
            lines:
                type: number";

        let conf = ConfigSpec::from_schema(schema).unwrap();
        match conf.validate(input) {
            Ok(_) => panic!("expected error, none received"),
            Err(_) => {},
        }
    }
}