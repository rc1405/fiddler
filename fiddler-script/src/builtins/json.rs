//! JSON processing built-in functions.

use crate::error::RuntimeError;
use crate::Value;

/// Parse JSON from bytes.
///
/// # Arguments
/// - Bytes containing valid JSON
///
/// # Returns
/// - Parsed value (string, integer, boolean, or null)
pub fn builtin_parse_json(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let bytes = match &args[0] {
        Value::Bytes(b) => b.clone(),
        Value::String(s) => s.as_bytes().to_vec(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "parse_json() requires bytes or string argument".to_string(),
            ))
        }
    };

    let json_value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| RuntimeError::InvalidArgument(format!("Invalid JSON: {}", e)))?;

    Ok(json_to_value(json_value))
}

/// Convert a serde_json::Value to a FiddlerScript Value.
pub fn json_to_value(json: serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Boolean(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                // Convert float to integer (truncate)
                Value::Integer(f as i64)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(arr) => Value::Array(arr.into_iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => {
            let dict: std::collections::HashMap<String, Value> = obj
                .into_iter()
                .map(|(k, v)| (k, json_to_value(v)))
                .collect();
            Value::Dictionary(dict)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_parse_json_string() {
        let json = br#""hello""#.to_vec();
        let result = builtin_parse_json(vec![Value::Bytes(json)]).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_builtin_parse_json_number() {
        let json = b"42".to_vec();
        let result = builtin_parse_json(vec![Value::Bytes(json)]).unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_builtin_parse_json_boolean() {
        let json = b"true".to_vec();
        let result = builtin_parse_json(vec![Value::Bytes(json)]).unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    #[test]
    fn test_builtin_parse_json_null() {
        let json = b"null".to_vec();
        let result = builtin_parse_json(vec![Value::Bytes(json)]).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_builtin_parse_json_from_string() {
        let result =
            builtin_parse_json(vec![Value::String(r#"{"key": "value"}"#.to_string())]).unwrap();
        assert!(matches!(result, Value::Dictionary(_)));
        if let Value::Dictionary(dict) = result {
            assert_eq!(dict.get("key"), Some(&Value::String("value".to_string())));
        }
    }

    #[test]
    fn test_builtin_parse_json_invalid() {
        let result = builtin_parse_json(vec![Value::Bytes(b"not valid json".to_vec())]);
        assert!(matches!(result, Err(RuntimeError::InvalidArgument(_))));
    }
}
