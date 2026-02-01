use handlebars::Handlebars;
use jsonschema::{Draft, JSONSchema};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use tracing::{debug, error, trace};

use core::future::Future;
use std::pin::Pin;

use super::{Error, Input, Metrics, Output, Processor};
use crate::{InputBatch, OutputBatch};

mod registration;
mod validate;
pub use registration::register_plugin;
pub(crate) use validate::parse_configuration_item;

/// Callback provides the pinned async function that will create the module
/// being supplied to the fiddler runtime
pub type Callback = fn(Value) -> Pin<Box<dyn Future<Output = Result<ExecutionType, Error>> + Send>>;

/// Plugin Configuration Type utilized for registration of fiddler modules
#[derive(PartialEq, Eq, Hash, Clone)]
pub enum ItemType {
    /// [crate::Input] trait enum variant
    Input,
    /// [crate::InputBatch] trait enum variant
    InputBatch,
    /// [crate::Output] trait enum variant
    Output,
    /// [crate::OutputBatch] trait enum variant
    OutputBatch,
    /// [crate::Processor] trait enum variant
    Processor,
    /// [crate::Metrics] backend enum variant
    Metrics,
}

impl fmt::Display for ItemType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let msg = match self {
            ItemType::Input => "input",
            ItemType::InputBatch => "input",
            ItemType::Output => "output",
            ItemType::OutputBatch => "output",
            ItemType::Processor => "processors",
            ItemType::Metrics => "metrics",
        };
        write!(f, "{}", msg)
    }
}

/// Enum for holding the implementation of the plugin trait to be called during processing
pub enum ExecutionType {
    /// [crate::Input] trait enum variant
    Input(Box<dyn Input + Send + Sync>),
    /// [crate::InputBatch] trait enum variant
    InputBatch(Box<dyn InputBatch + Send + Sync>),
    /// [crate::Output] trait enum variant
    Output(Box<dyn Output + Send + Sync>),
    /// [crate::OutputBatch] trait enum variant
    OutputBatch(Box<dyn OutputBatch + Send + Sync>),
    /// [crate::Processor] trait enum variant
    Processor(Box<dyn Processor + Send + Sync>),
    /// Metrics backend enum variant
    Metrics(Box<dyn Metrics + Send + Sync>),
}

/// Plugin registry using RwLock for concurrent read access after initialization.
/// Writes only occur during plugin registration at startup.
static ENV: Lazy<RwLock<HashMap<ItemType, HashMap<String, RegisteredItem>>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(ItemType::Input, HashMap::new());
    m.insert(ItemType::InputBatch, HashMap::new());
    m.insert(ItemType::Output, HashMap::new());
    m.insert(ItemType::OutputBatch, HashMap::new());
    m.insert(ItemType::Processor, HashMap::new());
    m.insert(ItemType::Metrics, HashMap::new());
    RwLock::new(m)
});

/// Parsed and validated configuration item
#[derive(Clone)]
pub(crate) struct RegisteredItem {
    pub creator: Callback,
    pub format: ConfigSpec,
}

/// Execution placeholder of the plugin to be used during processing
#[derive(Clone)]
pub(crate) struct ParsedRegisteredItem {
    pub creator: Callback,
    pub config: Value,
}

/// Unparsed configuration item used prior to validation
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Item {
    pub label: Option<String>,

    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Metrics configuration for observability.
///
/// Uses the same dynamic configuration pattern as inputs/processors/outputs,
/// allowing any registered metrics backend to be configured.
///
/// # Example Configuration
///
/// ```yaml
/// metrics:
///   interval: 300  # Record metrics every 5 minutes (default)
///   prometheus: {}
/// ```
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct MetricsConfig {
    /// Optional label for the metrics configuration
    pub label: Option<String>,

    /// Interval in seconds at which metrics are recorded (default: 300)
    #[serde(default = "MetricsConfig::default_interval")]
    pub interval: u64,

    /// Dynamic configuration for the metrics backend
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl MetricsConfig {
    /// Default metrics recording interval (300 seconds)
    fn default_interval() -> u64 {
        300
    }
}

/// Unparsed fiddler configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    /// Optional string label for the pipeline
    pub label: Option<String>,
    /// Number of threads to use for Processors and Outputs
    pub num_threads: Option<usize>,
    /// Optional metrics configuration for observability
    pub metrics: Option<MetricsConfig>,
    /// Input configuration following [crate::Input] or [crate::InputBatch] traits
    #[allow(private_interfaces)]
    pub input: Item,
    /// Processor configuration following [crate::Processor] traits
    #[allow(private_interfaces)]
    pub processors: Vec<Item>,
    /// Input configuration following [crate::Output] or [crate::OutputBatch] traits
    #[allow(private_interfaces)]
    pub output: Item,
}

impl FromStr for Config {
    type Err = Error;
    fn from_str(conf: &str) -> Result<Self, Self::Err> {
        Self::from_env(conf)
    }
}

impl Config {
    /// Core parsing logic that substitutes variables from the provided map.
    fn parse_with_variables(
        conf: &str,
        variables: &HashMap<String, String>,
    ) -> Result<Self, Error> {
        let mut handle_bars = Handlebars::new();
        handle_bars.set_strict_mode(true);

        let populated_config = handle_bars.render_template(conf, variables).map_err(|e| {
            Error::ConfigFailedValidation(format!(
                "Handlebars template error: {e}. Check your variable interpolations."
            ))
        })?;

        let config: Config = serde_yaml::from_str(&populated_config).map_err(|e| {
            Error::ConfigFailedValidation(format!(
                "YAML parsing error after variable substitution: {e}"
            ))
        })?;

        Ok(config)
    }

    /// Parse configuration with variable substitution from environment variables.
    ///
    /// This is equivalent to using `Config::from_str()` but provides a more
    /// explicit API for environment-based substitution.
    ///
    /// # Example
    /// ```no_run
    /// use fiddler::config::Config;
    ///
    /// let conf_str = r#"input:
    ///   file:
    ///     filename: {{INPUT_FILE}}
    /// processors: []
    /// output:
    ///   stdout: {}"#;
    ///
    /// // Uses environment variables for substitution
    /// let config = Config::from_env(conf_str).unwrap();
    /// ```
    pub fn from_env(conf: &str) -> Result<Self, Error> {
        let environment_variables: HashMap<String, String> = env::vars().collect();
        Self::parse_with_variables(conf, &environment_variables)
    }

    /// Parse configuration with variable substitution from provided variables.
    ///
    /// This allows programmatic control over variable values without requiring
    /// them to be set as environment variables.
    ///
    /// # Example
    /// ```
    /// use fiddler::config::Config;
    /// use std::collections::HashMap;
    ///
    /// let conf_str = r#"input:
    ///   stdin: {}
    /// processors: []
    /// output:
    ///   stdout: {}"#;
    ///
    /// let vars = HashMap::new();
    /// let config = Config::from_variables(conf_str, vars).unwrap();
    /// ```
    pub fn from_variables(conf: &str, variables: HashMap<String, String>) -> Result<Self, Error> {
        Self::parse_with_variables(conf, &variables)
    }

    /// Parse configuration with variable substitution from both environment
    /// variables and provided variables.
    ///
    /// Provided variables take precedence over environment variables when
    /// there are conflicts.
    ///
    /// # Example
    /// ```no_run
    /// use fiddler::config::Config;
    /// use std::collections::HashMap;
    ///
    /// let conf_str = r#"input:
    ///   file:
    ///     filename: {{INPUT_FILE}}
    ///     codec: {{CODEC}}
    /// processors: []
    /// output:
    ///   stdout: {}"#;
    ///
    /// let mut overrides = HashMap::new();
    /// overrides.insert("INPUT_FILE".to_string(), "/override/path.txt".to_string());
    ///
    /// // CODEC comes from env, INPUT_FILE uses the override
    /// let config = Config::from_env_with_overrides(conf_str, overrides).unwrap();
    /// ```
    pub fn from_env_with_overrides(
        conf: &str,
        overrides: HashMap<String, String>,
    ) -> Result<Self, Error> {
        let mut variables: HashMap<String, String> = env::vars().collect();
        variables.extend(overrides);
        Self::parse_with_variables(conf, &variables)
    }
}

impl Config {
    /// Validates the configuration object has valid and registered inputs, outputs, and processors.
    /// Note: Plugins must be registered with the environment prior to calling validate.  This is
    /// automatically done when using Environment
    /// ```compile_fail
    /// # use fiddler::config::Config;
    /// # use serde_yaml;
    /// # use fiddler::modules::register_plugins;
    /// # register_plugins().unwrap();
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
    pub async fn validate(self) -> Result<ParsedConfig, Error> {
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

        let input = match parse_configuration_item(ItemType::Input, &self.input.extra).await {
            Ok(i) => i,
            Err(e) => match e {
                Error::ConfigurationItemNotFound(_) => {
                    parse_configuration_item(ItemType::InputBatch, &self.input.extra).await?
                }
                _ => return Err(e),
            },
        };

        let output = match parse_configuration_item(ItemType::Output, &self.output.extra).await {
            Ok(i) => i,
            Err(e) => match e {
                Error::ConfigurationItemNotFound(_) => {
                    parse_configuration_item(ItemType::OutputBatch, &self.output.extra).await?
                }
                _ => return Err(e),
            },
        };

        let mut processors = Vec::new();

        for p in &self.processors {
            let proc = parse_configuration_item(ItemType::Processor, &p.extra).await?;
            processors.push(proc);
        }

        let num_threads = self.num_threads.unwrap_or(num_cpus::get());
        trace!("Num threads are {}", num_threads);

        let label = self.label.clone();
        let metrics = self.metrics.clone();
        debug!("configuration is valid");

        Ok(ParsedConfig {
            label,
            input,
            processors,
            num_threads,
            metrics,
            output,
        })
    }
}

/// Parsed and validated fiddler configuration
#[derive(Clone)]
pub struct ParsedConfig {
    /// Optional string label for the pipeline
    pub label: Option<String>,
    /// Number of threads to use for Processors and Outputs
    pub num_threads: usize,
    /// Optional metrics configuration for observability
    pub metrics: Option<MetricsConfig>,
    /// Input configuration following [crate::Input] or [crate::InputBatch] traits
    #[allow(private_interfaces)]
    pub input: ParsedRegisteredItem,
    /// Processor configuration following [crate::Processor] traits
    #[allow(private_interfaces)]
    pub processors: Vec<ParsedRegisteredItem>,
    /// Input configuration following [crate::Output] or [crate::OutputBatch] traits
    #[allow(private_interfaces)]
    pub output: ParsedRegisteredItem,
}

/// Plugin configuration validation snippet
///
/// Uses `Arc` internally to make cloning cheap without re-parsing the schema.
pub struct ConfigSpec {
    raw_schema: String,
    schema: Arc<JSONSchema>,
}

/// Schemas organized by category and plugin name.
///
/// The outer HashMap maps category names (e.g., "input", "output") to
/// inner HashMaps that map plugin names to their JSON schema strings.
pub type SchemaExport = HashMap<String, HashMap<String, String>>;

/// Exports all registered plugin schemas organized by plugin type.
///
/// This function collects JSON schemas from all registered plugins and returns them
/// in a hierarchical structure for documentation, validation, or tooling purposes.
///
/// # Returns
///
/// A [`SchemaExport`] where:
/// - The outer key is the plugin category name (e.g., "input", "output", "processors", "metrics")
/// - The inner `HashMap` maps plugin names to their JSON schema strings
///
/// # Errors
///
/// Returns an error if:
/// - Plugin registration fails
/// - Unable to acquire read lock on the plugin registry ([`Error::UnableToSecureLock`])
///
/// # Examples
///
/// ```no_run
/// use fiddler::config::export_schemas;
///
/// let schemas = export_schemas().unwrap();
/// if let Some(inputs) = schemas.get("input") {
///     for (name, schema) in inputs {
///         println!("Input plugin '{}' has schema: {}", name, schema);
///     }
/// }
/// ```
pub fn export_schemas() -> Result<SchemaExport, Error> {
    // Ensure all plugins are registered before exporting
    crate::modules::register_plugins()?;

    // Acquire read lock on the plugin registry
    let lock = ENV.read().map_err(|_| Error::UnableToSecureLock)?;

    // Define the plugin types to export with their corresponding export names
    let plugin_types = [
        (ItemType::Input, "input"),
        (ItemType::InputBatch, "input_batch"),
        (ItemType::Processor, "processors"),
        (ItemType::Metrics, "metrics"),
        (ItemType::Output, "output"),
        (ItemType::OutputBatch, "output_batch"),
    ];

    // Extract schemas for each plugin type
    let results: SchemaExport = plugin_types
        .iter()
        .filter_map(|(item_type, export_name)| {
            lock.get(item_type).map(|plugins| {
                let schemas: HashMap<String, String> = plugins
                    .iter()
                    .map(|(name, registered_item)| {
                        (name.clone(), registered_item.format.raw_schema.clone())
                    })
                    .collect();

                (export_name.to_string(), schemas)
            })
        })
        .collect();

    if results.is_empty() {
        debug!("No plugin schemas found in registry");
    }

    Ok(results)
}

impl std::fmt::Debug for ConfigSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigSpec")
            .field("raw_schema", &self.raw_schema)
            .finish()
    }
}

impl Clone for ConfigSpec {
    fn clone(&self) -> Self {
        ConfigSpec {
            raw_schema: self.raw_schema.clone(),
            schema: Arc::clone(&self.schema),
        }
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
            Err(e) => return Err(Error::InvalidValidationSchema(format!("{e}"))),
        };

        trace!("json schema is valid");

        Ok(ConfigSpec {
            raw_schema: conf.into(),
            schema: Arc::new(schema),
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
        if let Ok(_) = conf.validate(input) {
            panic!("expected error, none received")
        }
    }

    #[test]
    fn from_variables_uses_provided_map() {
        let conf_str = r#"input:
  stdin: {}
processors: []
output:
  file:
    path: {{OUTPUT_PATH}}"#;

        let mut vars = HashMap::new();
        vars.insert("OUTPUT_PATH".to_string(), "/test/output.txt".to_string());

        let config = Config::from_variables(conf_str, vars).unwrap();

        // Verify the variable was substituted correctly
        let output_yaml = serde_yaml::to_string(&config.output).unwrap();
        assert!(
            output_yaml.contains("/test/output.txt"),
            "Expected substituted path in output config"
        );
    }

    #[test]
    fn from_variables_does_not_use_env() {
        // Set an env var that we won't include in the provided HashMap
        std::env::set_var("FIDDLER_TEST_VAR_ISOLATED", "env_value");

        let conf_str = r#"input:
  stdin: {}
processors: []
output:
  file:
    path: {{FIDDLER_TEST_VAR_ISOLATED}}"#;

        // Provide an empty HashMap - should fail because strict mode is enabled
        let vars = HashMap::new();

        let result = Config::from_variables(conf_str, vars);
        assert!(
            result.is_err(),
            "Expected error when variable not in provided map"
        );

        // Clean up
        std::env::remove_var("FIDDLER_TEST_VAR_ISOLATED");
    }

    #[test]
    fn from_env_reads_environment_variables() {
        // Set a unique env var for this test
        std::env::set_var("FIDDLER_TEST_ENV_PATH", "/env/path.txt");

        let conf_str = r#"input:
  stdin: {}
processors: []
output:
  file:
    path: {{FIDDLER_TEST_ENV_PATH}}"#;

        let config = Config::from_env(conf_str).unwrap();

        let output_yaml = serde_yaml::to_string(&config.output).unwrap();
        assert!(
            output_yaml.contains("/env/path.txt"),
            "Expected env variable to be substituted"
        );

        // Clean up
        std::env::remove_var("FIDDLER_TEST_ENV_PATH");
    }

    #[test]
    fn from_env_with_overrides_prefers_overrides() {
        // Set an env var
        std::env::set_var("FIDDLER_TEST_OVERRIDE_PATH", "env_value");

        let conf_str = r#"input:
  stdin: {}
processors: []
output:
  file:
    path: {{FIDDLER_TEST_OVERRIDE_PATH}}"#;

        let mut overrides = HashMap::new();
        overrides.insert(
            "FIDDLER_TEST_OVERRIDE_PATH".to_string(),
            "override_value".to_string(),
        );

        let config = Config::from_env_with_overrides(conf_str, overrides).unwrap();

        let output_yaml = serde_yaml::to_string(&config.output).unwrap();
        assert!(
            output_yaml.contains("override_value"),
            "Expected override to take precedence over env"
        );
        assert!(
            !output_yaml.contains("env_value"),
            "Should not contain env value"
        );

        // Clean up
        std::env::remove_var("FIDDLER_TEST_OVERRIDE_PATH");
    }

    #[test]
    fn from_env_with_overrides_uses_env_when_no_override() {
        // Set env vars - one will be overridden, one won't
        std::env::set_var("FIDDLER_TEST_PATH_A", "/path/a");
        std::env::set_var("FIDDLER_TEST_PATH_B", "/path/b");

        let conf_str = r#"input:
  file:
    path: {{FIDDLER_TEST_PATH_A}}
    codec: Lines
processors: []
output:
  file:
    path: {{FIDDLER_TEST_PATH_B}}"#;

        let mut overrides = HashMap::new();
        // Only override PATH_A
        overrides.insert("FIDDLER_TEST_PATH_A".to_string(), "/override/a".to_string());

        let config = Config::from_env_with_overrides(conf_str, overrides).unwrap();

        let input_yaml = serde_yaml::to_string(&config.input).unwrap();
        let output_yaml = serde_yaml::to_string(&config.output).unwrap();

        assert!(
            input_yaml.contains("/override/a"),
            "PATH_A should use override"
        );
        assert!(
            output_yaml.contains("/path/b"),
            "PATH_B should use env value"
        );

        // Clean up
        std::env::remove_var("FIDDLER_TEST_PATH_A");
        std::env::remove_var("FIDDLER_TEST_PATH_B");
    }

    #[test]
    fn missing_variable_in_strict_mode_returns_error() {
        let conf_str = r#"input:
  stdin: {}
processors: []
output:
  file:
    path: {{UNDEFINED_VARIABLE_XYZ}}"#;

        // Ensure the variable doesn't exist
        std::env::remove_var("UNDEFINED_VARIABLE_XYZ");

        let result = Config::from_env(conf_str);
        assert!(result.is_err(), "Expected error for missing variable");

        if let Err(Error::ConfigFailedValidation(msg)) = result {
            assert!(
                msg.contains("Handlebars template error"),
                "Error should mention template error: {}",
                msg
            );
        } else {
            panic!("Expected ConfigFailedValidation error");
        }
    }

    #[test]
    fn from_str_uses_environment_variables() {
        // Verify that FromStr implementation delegates to from_env
        std::env::set_var("FIDDLER_FROM_STR_TEST", "/fromstr/path");

        let conf_str = r#"input:
  stdin: {}
processors: []
output:
  file:
    path: {{FIDDLER_FROM_STR_TEST}}"#;

        let config: Config = conf_str.parse().unwrap();

        let output_yaml = serde_yaml::to_string(&config.output).unwrap();
        assert!(
            output_yaml.contains("/fromstr/path"),
            "FromStr should use environment variables"
        );

        // Clean up
        std::env::remove_var("FIDDLER_FROM_STR_TEST");
    }
}
