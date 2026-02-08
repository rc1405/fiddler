use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::MessageBatch;
use crate::{Closer, Error, Processor};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;

#[derive(Deserialize, Serialize)]
struct Mapping {
    source: String,
    target: String,
}

#[derive(Deserialize, Serialize)]
struct TransformConfig {
    label: Option<String>,
    mappings: Vec<Mapping>,
}

pub struct Transform {
    mappings: Vec<Mapping>,
}

#[async_trait]
impl Processor for Transform {
    async fn process(&self, message: Message) -> Result<MessageBatch, Error> {
        let json_str = String::from_utf8(message.bytes.clone())
            .map_err(|e| Error::ProcessingError(format!("{e}")))?;

        let mut runtime = jmespath::Runtime::new();
        runtime.register_builtin_functions();

        let mut results = HashMap::new();
        for m in &self.mappings {
            let expr = runtime
                .compile(&m.source)
                .map_err(|e| Error::ProcessingError(format!("{e}")))?;

            let data = jmespath::Variable::from_json(&json_str)
                .map_err(|e| Error::ProcessingError(format!("{e}")))?;

            let result = expr
                .search(data)
                .map_err(|e| Error::ProcessingError(format!("{e}")))?;
            results.insert(&m.target, result);
        }

        let new_msg =
            serde_json::to_vec(&results).map_err(|e| Error::ProcessingError(format!("{e}")))?;

        Ok(vec![Message {
            bytes: new_msg,
            metadata: message.metadata.clone(),
            ..Default::default()
        }])
    }
}

#[async_trait]
impl Closer for Transform {
    async fn close(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_transform(conf: Value) -> Result<ExecutionType, Error> {
    let c: TransformConfig = serde_yaml::from_value(conf.clone())?;
    for m in &c.mappings {
        let _ = jmespath::compile(&m.source)
            .map_err(|e| Error::ConfigFailedValidation(format!("{e}")))?;
    }
    let s = Transform {
        mappings: c.mappings,
    };

    Ok(ExecutionType::Processor(Box::new(s)))
}

pub(super) fn register_transform() -> Result<(), Error> {
    let config = "type: object
properties:
  label:
    type: string
  mappings:
    type: array
    items:
      type: object
      properties:
        source:
          type: string
        target:
          type: string
required:
  - mappings";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "transform".into(),
        ItemType::Processor,
        conf_spec,
        create_transform,
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_transform().unwrap()
    }

    #[tokio::test]
    async fn test_simple_field_mapping() {
        let processor = Transform {
            mappings: vec![
                Mapping {
                    source: "name".to_string(),
                    target: "user_name".to_string(),
                },
                Mapping {
                    source: "age".to_string(),
                    target: "user_age".to_string(),
                },
            ],
        };

        let message = Message {
            bytes: br#"{"name": "Alice", "age": 30, "city": "NYC"}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        let output: serde_json::Value = serde_json::from_slice(&result[0].bytes).unwrap();
        assert_eq!(output["user_name"], "Alice");
        assert_eq!(output["user_age"], 30);
        // Original field "city" should not be in output
        assert!(output.get("city").is_none());
        assert!(output.get("name").is_none());
    }

    #[tokio::test]
    async fn test_nested_field_extraction() {
        let processor = Transform {
            mappings: vec![
                Mapping {
                    source: "user.profile.email".to_string(),
                    target: "email".to_string(),
                },
                Mapping {
                    source: "user.name".to_string(),
                    target: "name".to_string(),
                },
            ],
        };

        let message = Message {
            bytes: br#"{"user": {"name": "Bob", "profile": {"email": "bob@example.com", "phone": "555-1234"}}}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        let output: serde_json::Value = serde_json::from_slice(&result[0].bytes).unwrap();
        assert_eq!(output["email"], "bob@example.com");
        assert_eq!(output["name"], "Bob");
    }

    #[tokio::test]
    async fn test_array_extraction() {
        let processor = Transform {
            mappings: vec![
                Mapping {
                    source: "items[0]".to_string(),
                    target: "first_item".to_string(),
                },
                Mapping {
                    source: "items[-1]".to_string(),
                    target: "last_item".to_string(),
                },
            ],
        };

        let message = Message {
            bytes: br#"{"items": ["apple", "banana", "cherry"]}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        let output: serde_json::Value = serde_json::from_slice(&result[0].bytes).unwrap();
        assert_eq!(output["first_item"], "apple");
        assert_eq!(output["last_item"], "cherry");
    }

    #[tokio::test]
    async fn test_array_projection() {
        let processor = Transform {
            mappings: vec![Mapping {
                source: "users[*].name".to_string(),
                target: "names".to_string(),
            }],
        };

        let message = Message {
            bytes: br#"{"users": [{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]}"#
                .to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        let output: serde_json::Value = serde_json::from_slice(&result[0].bytes).unwrap();
        let names = output["names"].as_array().unwrap();
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "Alice");
        assert_eq!(names[1], "Bob");
    }

    #[tokio::test]
    async fn test_jmespath_function() {
        let processor = Transform {
            mappings: vec![
                Mapping {
                    source: "length(items)".to_string(),
                    target: "item_count".to_string(),
                },
                Mapping {
                    source: "join(', ', items)".to_string(),
                    target: "items_string".to_string(),
                },
            ],
        };

        let message = Message {
            bytes: br#"{"items": ["a", "b", "c"]}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        let output: serde_json::Value = serde_json::from_slice(&result[0].bytes).unwrap();
        assert_eq!(output["item_count"], 3);
        assert_eq!(output["items_string"], "a, b, c");
    }

    #[tokio::test]
    async fn test_null_value_extraction() {
        let processor = Transform {
            mappings: vec![
                Mapping {
                    source: "existing".to_string(),
                    target: "found".to_string(),
                },
                Mapping {
                    source: "nonexistent".to_string(),
                    target: "missing".to_string(),
                },
            ],
        };

        let message = Message {
            bytes: br#"{"existing": "value"}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        let output: serde_json::Value = serde_json::from_slice(&result[0].bytes).unwrap();
        assert_eq!(output["found"], "value");
        assert!(output["missing"].is_null());
    }

    #[tokio::test]
    async fn test_preserves_metadata() {
        let processor = Transform {
            mappings: vec![Mapping {
                source: "data".to_string(),
                target: "value".to_string(),
            }],
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "source".to_string(),
            serde_yaml::Value::String("test-source".to_string()),
        );

        let message = Message {
            bytes: br#"{"data": 42}"#.to_vec(),
            metadata: metadata.clone(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].metadata, metadata);
    }

    #[tokio::test]
    async fn test_complex_object_extraction() {
        let processor = Transform {
            mappings: vec![Mapping {
                source: "user".to_string(),
                target: "profile".to_string(),
            }],
        };

        let message = Message {
            bytes: br#"{"user": {"name": "Alice", "settings": {"theme": "dark"}}}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        let output: serde_json::Value = serde_json::from_slice(&result[0].bytes).unwrap();
        assert_eq!(output["profile"]["name"], "Alice");
        assert_eq!(output["profile"]["settings"]["theme"], "dark");
    }

    #[tokio::test]
    async fn test_invalid_json_error() {
        let processor = Transform {
            mappings: vec![Mapping {
                source: "field".to_string(),
                target: "output".to_string(),
            }],
        };

        let message = Message {
            bytes: b"not valid json".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await;
        assert!(matches!(result, Err(Error::ProcessingError(_))));
    }

    #[tokio::test]
    async fn test_invalid_utf8_error() {
        let processor = Transform {
            mappings: vec![Mapping {
                source: "field".to_string(),
                target: "output".to_string(),
            }],
        };

        let message = Message {
            bytes: vec![0xff, 0xfe, 0x00, 0x01],
            ..Default::default()
        };

        let result = processor.process(message).await;
        assert!(matches!(result, Err(Error::ProcessingError(_))));
    }

    #[tokio::test]
    async fn test_filter_expression() {
        let processor = Transform {
            mappings: vec![Mapping {
                source: "items[?price > `10`]".to_string(),
                target: "expensive_items".to_string(),
            }],
        };

        let message = Message {
            bytes: br#"{"items": [{"name": "A", "price": 5}, {"name": "B", "price": 15}, {"name": "C", "price": 25}]}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        let output: serde_json::Value = serde_json::from_slice(&result[0].bytes).unwrap();
        let expensive = output["expensive_items"].as_array().unwrap();
        assert_eq!(expensive.len(), 2);
        assert_eq!(expensive[0]["name"], "B");
        assert_eq!(expensive[1]["name"], "C");
    }

    #[tokio::test]
    async fn test_pipe_expression() {
        // users[*].scores returns [[90, 85], [75, 80]]
        // | [0] takes the first element: [90, 85]
        let processor = Transform {
            mappings: vec![Mapping {
                source: "users[*].scores | [0]".to_string(),
                target: "first_user_scores".to_string(),
            }],
        };

        let message = Message {
            bytes: br#"{"users": [{"scores": [90, 85]}, {"scores": [75, 80]}]}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        let output: serde_json::Value = serde_json::from_slice(&result[0].bytes).unwrap();
        let scores = output["first_user_scores"].as_array().unwrap();
        assert_eq!(scores.len(), 2);
        assert_eq!(scores[0], 90);
        assert_eq!(scores[1], 85);
    }

    #[tokio::test]
    async fn test_multiselect_hash() {
        let processor = Transform {
            mappings: vec![Mapping {
                source: "{full_name: name, user_email: email}".to_string(),
                target: "contact".to_string(),
            }],
        };

        let message = Message {
            bytes: br#"{"name": "Alice", "email": "alice@example.com", "phone": "555-1234"}"#
                .to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        let output: serde_json::Value = serde_json::from_slice(&result[0].bytes).unwrap();
        assert_eq!(output["contact"]["full_name"], "Alice");
        assert_eq!(output["contact"]["user_email"], "alice@example.com");
    }
}
