//! FiddlerScript processor for inline message manipulation.
//!
//! This processor uses the FiddlerScript scripting language to transform messages.

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::MessageBatch;
use crate::{Closer, Error, Processor};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use fiddler_script::{Interpreter, Value};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use std::collections::HashMap;

/// Configuration for the FiddlerScript processor.
#[derive(Clone, Deserialize, Serialize)]
pub struct FiddlerScriptSpec {
    /// The FiddlerScript code to execute
    code: String,
}

/// FiddlerScript processor implementation.
pub struct FiddlerScriptProcessor {
    /// The script code to execute
    code: String,
}

impl FiddlerScriptProcessor {
    /// Convert metadata from serde_yaml::Value to fiddler_script::Value
    fn convert_metadata(metadata: &HashMap<String, serde_yaml::Value>) -> IndexMap<String, Value> {
        metadata
            .iter()
            .map(|(k, v)| (k.clone(), Self::yaml_to_script_value(v)))
            .collect()
    }

    /// Convert a serde_yaml::Value to a fiddler_script::Value
    fn yaml_to_script_value(yaml: &serde_yaml::Value) -> Value {
        match yaml {
            serde_yaml::Value::Null => Value::Null,
            serde_yaml::Value::Bool(b) => Value::Boolean(*b),
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::Null
                }
            }
            serde_yaml::Value::String(s) => Value::String(s.clone()),
            serde_yaml::Value::Sequence(seq) => {
                Value::Array(seq.iter().map(Self::yaml_to_script_value).collect())
            }
            serde_yaml::Value::Mapping(map) => {
                let dict: IndexMap<String, Value> = map
                    .iter()
                    .filter_map(|(k, v)| {
                        k.as_str()
                            .map(|key| (key.to_string(), Self::yaml_to_script_value(v)))
                    })
                    .collect();
                Value::Dictionary(dict)
            }
            serde_yaml::Value::Tagged(tagged) => Self::yaml_to_script_value(&tagged.value),
        }
    }

    /// Convert a fiddler_script::Value back to serde_yaml::Value for metadata
    #[allow(dead_code)]
    fn script_to_yaml_value(value: &Value) -> serde_yaml::Value {
        match value {
            Value::Null => serde_yaml::Value::Null,
            Value::Boolean(b) => serde_yaml::Value::Bool(*b),
            Value::Integer(n) => serde_yaml::Value::Number((*n).into()),
            Value::Float(f) => serde_yaml::Value::Number(serde_yaml::Number::from(*f)),
            Value::String(s) => serde_yaml::Value::String(s.clone()),
            Value::Bytes(b) => {
                // Convert bytes to string if valid UTF-8, otherwise base64
                match String::from_utf8(b.clone()) {
                    Ok(s) => serde_yaml::Value::String(s),
                    Err(_) => serde_yaml::Value::String(base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        b,
                    )),
                }
            }
            Value::Array(arr) => {
                serde_yaml::Value::Sequence(arr.iter().map(Self::script_to_yaml_value).collect())
            }
            Value::Dictionary(dict) => {
                let mapping: serde_yaml::Mapping = dict
                    .iter()
                    .map(|(k, v)| {
                        (
                            serde_yaml::Value::String(k.clone()),
                            Self::script_to_yaml_value(v),
                        )
                    })
                    .collect();
                serde_yaml::Value::Mapping(mapping)
            }
        }
    }

    /// Extract bytes from a Value
    fn value_to_bytes(value: &Value) -> Vec<u8> {
        value.to_bytes()
    }

    /// Create a message from a Value and original metadata
    fn create_message(
        value: &Value,
        original_metadata: &HashMap<String, serde_yaml::Value>,
    ) -> Message {
        Message {
            bytes: Self::value_to_bytes(value),
            metadata: original_metadata.clone(),
            ..Default::default()
        }
    }
}

#[async_trait]
impl Processor for FiddlerScriptProcessor {
    async fn process(&self, message: Message) -> Result<MessageBatch, Error> {
        // Create a new interpreter for each message (clean state)
        let mut interpreter = Interpreter::new_without_env();

        // Set 'this' to the message bytes
        interpreter.set_variable_bytes("this", message.bytes.clone());

        // Convert metadata to a dictionary and set as 'metadata'
        let metadata_dict = Self::convert_metadata(&message.metadata);
        interpreter.set_variable_dict("metadata", metadata_dict);

        // Run the script
        interpreter
            .run(&self.code)
            .map_err(|e| Error::ProcessingError(format!("FiddlerScript error: {}", e)))?;

        // Get the result from 'this'
        let result = interpreter.get_value("this").ok_or_else(|| {
            Error::ProcessingError("'this' variable not found after script execution".to_string())
        })?;

        // Check if result is an array (multiple messages), null (filtered), or single value
        match &result {
            // Null explicitly filters the message - return empty batch
            Value::Null => Ok(vec![]),
            // Empty array explicitly filters the message - return empty batch
            Value::Array(arr) if arr.is_empty() => Ok(vec![]),
            // Non-empty array - multiple messages
            Value::Array(arr) => {
                let messages: Vec<Message> = arr
                    .iter()
                    .map(|v| Self::create_message(v, &message.metadata))
                    .collect();
                Ok(messages)
            }
            // Single message
            _ => Ok(vec![Self::create_message(&result, &message.metadata)]),
        }
    }
}

impl Closer for FiddlerScriptProcessor {}

#[fiddler_registration_func]
fn create_fiddlerscript(conf: YamlValue) -> Result<ExecutionType, Error> {
    let c: FiddlerScriptSpec = serde_yaml::from_value(conf)?;
    Ok(ExecutionType::Processor(Box::new(FiddlerScriptProcessor {
        code: c.code,
    })))
}

pub(super) fn register_fiddlerscript() -> Result<(), Error> {
    let config = r#"type: object
properties:
  code:
    type: string
required:
  - code"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "fiddlerscript".into(),
        ItemType::Processor,
        conf_spec,
        create_fiddlerscript,
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_fiddlerscript().unwrap()
    }

    #[tokio::test]
    async fn test_simple_passthrough() {
        let processor = FiddlerScriptProcessor {
            code: "// passthrough".to_string(),
        };

        let message = Message {
            bytes: b"hello world".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bytes, b"hello world");
    }

    #[tokio::test]
    async fn test_modify_message() {
        let processor = FiddlerScriptProcessor {
            code: r#"
                let text = bytes_to_string(this);
                this = bytes(text + " modified");
            "#
            .to_string(),
        };

        let message = Message {
            bytes: b"hello".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bytes, b"hello modified");
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let processor = FiddlerScriptProcessor {
            code: r#"
                this = array(bytes("one"), bytes("two"), bytes("three"));
            "#
            .to_string(),
        };

        let message = Message {
            bytes: b"original".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].bytes, b"one");
        assert_eq!(result[1].bytes, b"two");
        assert_eq!(result[2].bytes, b"three");
    }

    #[tokio::test]
    async fn test_access_metadata() {
        let processor = FiddlerScriptProcessor {
            code: r#"
                let source = get(metadata, "source");
                this = bytes(source);
            "#
            .to_string(),
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "source".to_string(),
            serde_yaml::Value::String("test-input".to_string()),
        );

        let message = Message {
            bytes: b"original".to_vec(),
            metadata,
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bytes, b"test-input");
    }

    #[tokio::test]
    async fn test_json_parsing() {
        let processor = FiddlerScriptProcessor {
            code: r#"
                let data = parse_json(this);
                let name = get(data, "name");
                this = bytes(name);
            "#
            .to_string(),
        };

        let message = Message {
            bytes: br#"{"name": "Alice", "age": 30}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bytes, b"Alice");
    }

    #[tokio::test]
    async fn test_split_lines() {
        let processor = FiddlerScriptProcessor {
            code: r#"
                let text = bytes_to_string(this);
                let lines = array();
                let current = "";
                for (let i = 0; i < len(text); i = i + 1) {
                    let char = get(text, i);
                    if (char == "\n") {
                        if (len(current) > 0) {
                            lines = push(lines, bytes(current));
                        }
                        current = "";
                    } else {
                        current = current + char;
                    }
                }
                if (len(current) > 0) {
                    lines = push(lines, bytes(current));
                }
                this = lines;
            "#
            .to_string(),
        };

        let message = Message {
            bytes: b"line1\nline2\nline3".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_float_metadata() {
        let processor = FiddlerScriptProcessor {
            code: r#"
                let price = get(metadata, "price");
                let tax = price * 0.1;
                this = bytes(str(tax));
            "#
            .to_string(),
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "price".to_string(),
            serde_yaml::Value::Number(serde_yaml::Number::from(99.99)),
        );

        let message = Message {
            bytes: b"original".to_vec(),
            metadata,
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);
        // 99.99 * 0.1 = 9.999
        let result_str = String::from_utf8(result[0].bytes.clone()).unwrap();
        assert!(result_str.starts_with("9.999"));
    }

    #[tokio::test]
    async fn test_float_arithmetic() {
        let processor = FiddlerScriptProcessor {
            code: r#"
                let a = 3.14;
                let b = 2.0;
                let result = a * b;
                this = bytes(str(result));
            "#
            .to_string(),
        };

        let message = Message {
            bytes: b"original".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);
        let result_str = String::from_utf8(result[0].bytes.clone()).unwrap();
        assert!(result_str.starts_with("6.28"));
    }

    #[tokio::test]
    async fn test_float_math_functions() {
        let processor = FiddlerScriptProcessor {
            code: r#"
                let x = 3.7;
                let c = ceil(x);
                let f = floor(x);
                let r = round(x);
                this = bytes(str(c) + "," + str(f) + "," + str(r));
            "#
            .to_string(),
        };

        let message = Message {
            bytes: b"original".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bytes, b"4,3,4");
    }

    #[tokio::test]
    async fn test_filter_with_null() {
        // Setting this to null filters the message
        let processor = FiddlerScriptProcessor {
            code: r#"
                this = null;
            "#
            .to_string(),
        };

        let message = Message {
            bytes: b"should be filtered".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(
            result.len(),
            0,
            "null should result in empty batch (filtered)"
        );
    }

    #[tokio::test]
    async fn test_filter_with_empty_array() {
        // Setting this to empty array filters the message
        let processor = FiddlerScriptProcessor {
            code: r#"
                this = array();
            "#
            .to_string(),
        };

        let message = Message {
            bytes: b"should be filtered".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(
            result.len(),
            0,
            "empty array should result in empty batch (filtered)"
        );
    }

    #[tokio::test]
    async fn test_conditional_filter() {
        // Filter messages based on content using null
        let processor = FiddlerScriptProcessor {
            code: r#"
                let text = bytes_to_string(this);
                if (text == "drop") {
                    this = null;
                }
            "#
            .to_string(),
        };

        // Message that should be filtered
        let message = Message {
            bytes: b"drop".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 0, "message 'drop' should be filtered");

        // Message that should pass through
        let message = Message {
            bytes: b"keep".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1, "message 'keep' should pass through");
        assert_eq!(result[0].bytes, b"keep");
    }
}
