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

#[derive(Deserialize, Serialize)]
struct FilterConfig {
    label: Option<String>,
    condition: String,
}

pub struct Filter {
    condition: String,
}

#[async_trait]
impl Processor for Filter {
    async fn process(&self, message: Message) -> Result<MessageBatch, Error> {
        let json_str = String::from_utf8(message.bytes.clone())
            .map_err(|e| Error::ProcessingError(format!("{e}")))?;

        let mut runtime = jmespath::Runtime::new();
        runtime.register_builtin_functions();
        let expr = runtime
            .compile(&self.condition)
            .map_err(|e| Error::ProcessingError(format!("{e}")))?;

        let data = jmespath::Variable::from_json(&json_str)
            .map_err(|e| Error::ProcessingError(format!("{e}")))?;

        let result = expr
            .search(data)
            .map_err(|e| Error::ProcessingError(format!("{e}")))?;

        // Explicitly check that result is a boolean type
        match result.as_boolean() {
            Some(true) => Ok(vec![message]),
            Some(false) => Ok(Vec::new()),
            None => Err(Error::ProcessingError(format!(
                "Filter '{}' did not return a boolean value, got: {:?}",
                self.condition, result
            ))),
        }
    }
}

#[async_trait]
impl Closer for Filter {
    async fn close(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_filter(conf: Value) -> Result<ExecutionType, Error> {
    let c: FilterConfig = serde_yaml::from_value(conf.clone())?;
    let _ = jmespath::compile(&c.condition)
        .map_err(|e| Error::ConfigFailedValidation(format!("{e}")))?;

    let s = Filter {
        condition: c.condition,
    };

    Ok(ExecutionType::Processor(Box::new(s)))
}

pub(super) fn register_filter() -> Result<(), Error> {
    let config = "type: object
properties:
  label:
    type: string
  condition:
    type: string
required:
  - condition";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "filter".into(),
        ItemType::Processor,
        conf_spec,
        create_filter,
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_filter().unwrap()
    }

    #[tokio::test]
    async fn test_filter_condition_true() {
        let processor = Filter {
            condition: "status == 'active'".to_string(),
        };

        let message = Message {
            bytes: br#"{"status": "active", "name": "test"}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message.clone()).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bytes, message.bytes);
    }

    #[tokio::test]
    async fn test_filter_condition_false() {
        let processor = Filter {
            condition: "status == 'active'".to_string(),
        };

        let message = Message {
            bytes: br#"{"status": "inactive", "name": "test"}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await;
        let expected: Vec<Message> = Vec::new();
        assert!(matches!(result, Ok(expected)));
    }

    #[tokio::test]
    async fn test_filter_numeric_comparison() {
        let processor = Filter {
            condition: "age >= `18`".to_string(),
        };

        // Should pass - age is 21
        let message = Message {
            bytes: br#"{"name": "Alice", "age": 21}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message.clone()).await.unwrap();
        assert_eq!(result.len(), 1);

        // Should fail - age is 16
        let message = Message {
            bytes: br#"{"name": "Bob", "age": 16}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await;
        let expected: Vec<Message> = Vec::new();
        assert!(matches!(result, Ok(expected)));
    }

    #[tokio::test]
    async fn test_filter_nested_field() {
        let processor = Filter {
            condition: "user.verified == `true`".to_string(),
        };

        let message = Message {
            bytes: br#"{"user": {"name": "Alice", "verified": true}}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message.clone()).await.unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn test_filter_array_contains() {
        let processor = Filter {
            condition: "contains(tags, 'important')".to_string(),
        };

        // Should pass - contains 'important'
        let message = Message {
            bytes: br#"{"tags": ["urgent", "important", "review"]}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        // Should fail - doesn't contain 'important'
        let message = Message {
            bytes: br#"{"tags": ["normal", "review"]}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await;
        let expected: Vec<Message> = Vec::new();
        assert!(matches!(result, Ok(expected)));
    }

    #[tokio::test]
    async fn test_filter_length_check() {
        let processor = Filter {
            condition: "length(items) > `0`".to_string(),
        };

        // Should pass - has items
        let message = Message {
            bytes: br#"{"items": [1, 2, 3]}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        // Should fail - empty array
        let message = Message {
            bytes: br#"{"items": []}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await;
        let expected: Vec<Message> = Vec::new();
        assert!(matches!(result, Ok(expected)));
    }

    #[tokio::test]
    async fn test_filter_null_check() {
        let processor = Filter {
            condition: "error != null".to_string(),
        };

        // Should pass - error is not null
        let message = Message {
            bytes: br#"{"error": "something went wrong"}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        // Should fail - error is null
        let message = Message {
            bytes: br#"{"error": null}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await;
        let expected: Vec<Message> = Vec::new();
        assert!(matches!(result, Ok(expected)));
    }

    #[tokio::test]
    async fn test_filter_non_boolean_result_error() {
        let processor = Filter {
            condition: "name".to_string(), // Returns a string, not boolean
        };

        let message = Message {
            bytes: br#"{"name": "Alice"}"#.to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await;
        assert!(matches!(result, Err(Error::ProcessingError(_))));
    }

    #[tokio::test]
    async fn test_filter_invalid_json_error() {
        let processor = Filter {
            condition: "status == 'active'".to_string(),
        };

        let message = Message {
            bytes: b"not valid json".to_vec(),
            ..Default::default()
        };

        let result = processor.process(message).await;
        assert!(matches!(result, Err(Error::ProcessingError(_))));
    }

    #[tokio::test]
    async fn test_filter_invalid_utf8_error() {
        let processor = Filter {
            condition: "status == 'active'".to_string(),
        };

        let message = Message {
            bytes: vec![0xff, 0xfe, 0x00, 0x01], // Invalid UTF-8
            ..Default::default()
        };

        let result = processor.process(message).await;
        assert!(matches!(result, Err(Error::ProcessingError(_))));
    }

    #[tokio::test]
    async fn test_filter_complex_condition() {
        let processor = Filter {
            condition: "type == 'order' && total > `100` && status != 'cancelled'".to_string(),
        };

        // Should pass - all conditions met
        let message = Message {
            bytes: br#"{"type": "order", "total": 150, "status": "pending"}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        // Should fail - total too low
        let message = Message {
            bytes: br#"{"type": "order", "total": 50, "status": "pending"}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await;
        let expected: Vec<Message> = Vec::new();
        assert!(matches!(result, Ok(expected)));

        // Should fail - cancelled
        let message = Message {
            bytes: br#"{"type": "order", "total": 150, "status": "cancelled"}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await;
        assert!(matches!(result, Ok(expected)));
    }

    #[tokio::test]
    async fn test_filter_starts_with() {
        let processor = Filter {
            condition: "starts_with(name, 'prod-')".to_string(),
        };

        // Should pass
        let message = Message {
            bytes: br#"{"name": "prod-server-01"}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await.unwrap();
        assert_eq!(result.len(), 1);

        // Should fail
        let message = Message {
            bytes: br#"{"name": "dev-server-01"}"#.to_vec(),
            ..Default::default()
        };
        let result = processor.process(message).await;
        let expected: Vec<Message> = Vec::new();
        assert!(matches!(result, Ok(expected)));
    }
}
