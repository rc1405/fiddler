use std::collections::HashMap;
use std::sync::Mutex;
use std::fmt;

use jsonschema::{Draft, JSONSchema};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use tracing::{debug, error, trace};

use crate::{InputBatch, OutputBatch};
use super::{Input, Output, Processor, Error};

mod registration;
mod validate;
pub use validate::parse_configuration_item;
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
    pub label: Option<String>,
    pub input: Item,
    pub pipeline: Pipeline,
    pub output: Item,
}

impl Config {
    

    pub fn validate(self) -> Result<ParsedConfig, Error> {
        if self.input.extra.len() > 1 {
            error!("input must only contain one entry");
            return Err(Error::Validation("input must only contain one entry".into()))
        };

        if self.output.extra.len() > 1 {
            error!("output must only contain one entry");
            return Err(Error::Validation("output must only contain one entry".into()))
        };

        if self.pipeline.processors.len() == 0 {
            error!("pipeline must contain at least one processor");
            return Err(Error::Validation("pipeline must contain at least one processor".into()))
        };
        
        let input = parse_configuration_item(ItemType::Input, &self.input.extra)?;

        let output = parse_configuration_item(ItemType::Output, &self.output.extra)?;

        let mut processors = Vec::new();

        for p in &self.pipeline.processors {
            let proc = parse_configuration_item(ItemType::Processor, &p.extra)?;
            processors.push(proc);
        };

        let parsed_pipeline = ParsedPipeline{
            label: self.pipeline.label.clone(),
            processors,
        };
        
        let label = self.label.clone();
        debug!("configuration is valid");

        Ok(ParsedConfig{
            _config: self,
            label,
            input,
            pipeline: parsed_pipeline,
            output,
        })
    }
}

pub struct ParsedConfig {
    _config: Config,
    pub label: Option<String>,
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

        trace!("json schema is valid");

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
            error!(number_of_failures = errs.len(), errors = errs.join(" "), "validation failed");
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