use std::collections::HashMap;
use std::env;
use std::fmt;
use std::str::FromStr;
use std::sync::Mutex;

use handlebars::Handlebars;
use jsonschema::{Draft, JSONSchema};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use tracing::{debug, error, trace};

use super::{Error, Input, Output, Processor};
use crate::{InputBatch, OutputBatch};

mod registration;
mod validate;
pub use registration::register_plugin;
pub use validate::parse_configuration_item;

type Callback = fn(&Value) -> Result<ExecutionType, Error>;

/// Plugin Configuration Type
#[derive(PartialEq, Eq, Hash, Clone)]
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
            ItemType::InputBatch => "input",
            ItemType::Output => "output",
            ItemType::OutputBatch => "output",
            ItemType::Processor => "processors",
        };
        write!(f, "{}", msg)
    }
}

/// Enum for holding the implementation of the plugin trait to be called during processing
// #[derive(Clone)]
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

/// Parsed and validated configuration item
#[derive(Clone)]
pub struct RegisteredItem {
    pub creator: Callback,
    pub format: ConfigSpec,
}

/// Execution placeholder of the plugin to be used during processing
#[derive(Clone)]
pub struct ParsedRegisteredItem {
    pub creator: Callback,
    pub config: Value,
}

/// Unparsed configuration item used prior to validation
#[derive(Debug, Deserialize, Serialize)]
pub struct Item {
    pub label: Option<String>,

    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Unparsed fiddler configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub label: Option<String>,
    pub num_threads: Option<usize>,
    pub input: Item,
    pub processors: Vec<Item>,
    pub output: Item,
}

impl FromStr for Config {
    type Err = Error;
    fn from_str(conf: &str) -> Result<Self, Self::Err> {
        let mut environment_variables: HashMap<String, String> = HashMap::new();
        for (key, value) in env::vars() {
            let _ = environment_variables.insert(key, value);
        }

        let mut handle_bars = Handlebars::new();
        handle_bars.set_strict_mode(true);

        let populated_config = handle_bars
            .render_template(conf, &environment_variables)
            .map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;

        let config: Config = serde_yaml::from_str(&populated_config)?;
        Ok(config)
    }
}

impl Config {
    /// Validates the configuration object has valid and registered inputs, outputs, and processors.
    /// Note: Plugins must be registered with the environment prior to calling validate.  This is
    /// automatically done when using Environment
    /// ```
    /// # use fiddler::config::Config;
    /// # use serde_yaml;
    /// # use fiddler::modules::{inputs, outputs, processors};
    /// # inputs::register_plugins().unwrap();
    /// # outputs::register_plugins().unwrap();
    /// # processors::register_plugins().unwrap();
    /// let conf_str = r#"input:
    ///   stdin: {}
    /// processors:
    ///   - noop: {}
    /// output:
    ///   stdout: {}"#;
    ///
    /// let config: Config = serde_yaml::from_str(&conf_str).unwrap();
    /// config.validate().unwrap();
    /// ```
    pub fn validate(self) -> Result<ParsedConfig, Error> {
        if self.input.extra.len() > 1 {
            error!("input must only contain one entry");
            return Err(Error::Validation(
                "input must only contain one entry".into(),
            ));
        };

        if self.output.extra.len() > 1 {
            error!("output must only contain one entry");
            return Err(Error::Validation(
                "output must only contain one entry".into(),
            ));
        };

        let input = parse_configuration_item(ItemType::Input, &self.input.extra)?;

        let output = match parse_configuration_item(ItemType::Output, &self.output.extra) {
            Ok(i) => i,
            Err(e) => match e {
                Error::ConfigurationItemNotFound(_) => {
                    parse_configuration_item(ItemType::OutputBatch, &self.output.extra)?
                }
                _ => return Err(e),
            },
        };

        let mut processors = Vec::new();

        for p in &self.processors {
            let proc = parse_configuration_item(ItemType::Processor, &p.extra)?;
            processors.push(proc);
        }

        let num_threads = self.num_threads.unwrap_or(num_cpus::get());
        trace!("Num threads are {}", num_threads);

        let label = self.label.clone();
        debug!("configuration is valid");

        Ok(ParsedConfig {
            label,
            input,
            processors,
            num_threads,
            output,
        })
    }
}

/// Parsed and validated fiddler configuration
#[derive(Clone)]
pub struct ParsedConfig {
    pub label: Option<String>,
    pub input: ParsedRegisteredItem,
    pub processors: Vec<ParsedRegisteredItem>,
    pub num_threads: usize,
    pub output: ParsedRegisteredItem,
}

/// Plugin configuration validation snippet
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
    /// Creates a snippet validation logic from the provided schema.  The schema is
    /// jsonschema format, in yaml.  Rather than using yamlschema validation directly
    /// this is converted to json and used with the jsonschema library.
    /// For the following input format:
    /// ```yaml
    /// scanner:
    ///   lines: true
    /// ```
    ///
    /// The following code would provide the code validation snippet.
    /// ```
    /// # use fiddler::config::ConfigSpec;
    /// # use serde_yaml;
    /// let conf_str = r#"properties:
    ///   scanner:
    ///     type: object
    ///     properties:
    ///       lines:
    ///         type: boolean"#;
    ///
    /// let config= ConfigSpec::from_schema(&conf_str).unwrap();
    /// ```
    pub fn from_schema(conf: &str) -> Result<Self, Error> {
        let v: Value = serde_yaml::from_str(conf)?;
        let intermediate = serde_json::to_string(&v)?;
        let f: serde_json::Value = serde_json::from_str(&intermediate)?;

        let schema: JSONSchema = match JSONSchema::options().with_draft(Draft::Draft7).compile(&f) {
            Ok(js) => js,
            Err(e) => return Err(Error::InvalidValidationSchema(format!("{}", e))),
        };

        trace!("json schema is valid");

        Ok(ConfigSpec {
            raw_schema: conf.into(),
            schema,
        })
    }

    /// Validates the configuration str against the validation schema provided to establish the
    /// ConfigSpec
    ///
    /// The following code would provide the code validation snippet.
    /// ```
    /// # use fiddler::config::ConfigSpec;
    /// # use serde_yaml;
    /// # let schema_str = r#"properties:
    /// #  scanner:
    /// #    type: object
    /// #    properties:
    /// #      lines:
    /// #        type: boolean"#;
    /// # let config = ConfigSpec::from_schema(&schema_str).unwrap();
    /// let config_str = r#"scanner:
    ///   lines: true"#;
    /// config.validate(config_str).unwrap();
    /// ```
    pub fn validate(&self, content: &str) -> Result<(), Error> {
        let v: Value = serde_yaml::from_str(content)?;
        let intermediate = serde_json::to_string(&v)?;
        let f: serde_json::Value = serde_json::from_str(&intermediate)?;
        let result = self.schema.validate(&f);
        if let Err(errors) = result {
            let errs: Vec<String> = errors.into_iter().map(|i| format!("{}", i)).collect();
            error!(
                number_of_failures = errs.len(),
                errors = errs.join(" "),
                "validation failed"
            );
            return Err(Error::ConfigFailedValidation(errs.join(" ")));
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
            Err(_) => {}
        }
    }
}
