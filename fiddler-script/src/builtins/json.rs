//! JSON processing built-in functions.

use base64::Engine;
use indexmap::IndexMap;

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
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    let bytes = match &args[0] {
        Value::Bytes(b) => b.clone(),
        Value::String(s) => s.as_bytes().to_vec(),
        _ => {
            return Err(RuntimeError::invalid_argument(
                "parse_json() requires bytes or string argument".to_string(),
            ))
        }
    };

    let json_value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| RuntimeError::invalid_argument(format!("Invalid JSON: {}", e)))?;

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
            let dict: IndexMap<String, Value> = obj
                .into_iter()
                .map(|(k, v)| (k, json_to_value(v)))
                .collect();
            Value::Dictionary(dict)
        }
    }
}

/// Convert a FiddlerScript Value to a serde_json::Value.
pub fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Boolean(b) => serde_json::Value::Bool(*b),
        Value::Integer(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
        Value::Float(f) => {
            if let Some(num) = serde_json::Number::from_f64(*f) {
                serde_json::Value::Number(num)
            } else {
                serde_json::Value::Null
            }
        }
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Bytes(b) => {
            // Convert bytes to base64 string for JSON representation
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(b))
        }
        Value::Array(arr) => serde_json::Value::Array(arr.iter().map(value_to_json).collect()),
        Value::Dictionary(dict) => {
            let obj: serde_json::Map<String, serde_json::Value> = dict
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
    }
}

/// Query JSON data using JMESPath expressions.
///
/// # Arguments
/// - Value (dictionary or array) - The JSON data to query
/// - String - The JMESPath expression
///
/// # Returns
/// - Extracted value, or null if not found
///
/// # Example
/// ```ignore
/// let raw = parse_json(this);
/// let extracted = jmespath(raw, "embedded.Key");
/// ```
pub fn builtin_jmespath(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::wrong_argument_count(2, args.len()));
    }

    // Get the JMESPath expression string
    let expression = match &args[1] {
        Value::String(s) => s.as_str(),
        _ => {
            return Err(RuntimeError::invalid_argument(
                "jmespath() requires a string expression as second argument".to_string(),
            ))
        }
    };

    // Convert FiddlerScript Value to serde_json::Value
    let json_data = value_to_json(&args[0]);

    // Compile the JMESPath expression
    let expr = jmespath::compile(expression).map_err(|e| {
        RuntimeError::invalid_argument(format!("Invalid JMESPath expression: {}", e))
    })?;

    // Convert serde_json::Value to JMESPath Variable
    let data = jmespath::Variable::from_serializable(&json_data).map_err(|e| {
        RuntimeError::invalid_argument(format!(
            "Failed to convert data to JMESPath variable: {}",
            e
        ))
    })?;

    // Execute the search
    let result = expr.search(&data).map_err(|e| {
        RuntimeError::invalid_argument(format!("JMESPath query failed: {}", e))
    })?;

    // Convert result back to serde_json::Value, then to FiddlerScript Value
    // The result is an Rc<Variable>, dereference to get the Variable
    let result_json: serde_json::Value = serde_json::to_value(&*result).map_err(|e| {
        RuntimeError::invalid_argument(format!("Failed to convert JMESPath result: {}", e))
    })?;

    Ok(json_to_value(result_json))
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
        assert!(matches!(result, Err(RuntimeError::InvalidArgument { .. })));
    }

    #[test]
    fn test_value_to_json_null() {
        let value = Value::Null;
        let json = value_to_json(&value);
        assert_eq!(json, serde_json::Value::Null);
    }

    #[test]
    fn test_value_to_json_boolean() {
        let value = Value::Boolean(true);
        let json = value_to_json(&value);
        assert_eq!(json, serde_json::Value::Bool(true));
    }

    #[test]
    fn test_value_to_json_integer() {
        let value = Value::Integer(42);
        let json = value_to_json(&value);
        assert_eq!(json, serde_json::json!(42));
    }

    #[test]
    fn test_value_to_json_float() {
        let value = Value::Float(3.14);
        let json = value_to_json(&value);
        assert_eq!(json, serde_json::json!(3.14));
    }

    #[test]
    fn test_value_to_json_string() {
        let value = Value::String("hello".to_string());
        let json = value_to_json(&value);
        assert_eq!(json, serde_json::Value::String("hello".to_string()));
    }

    #[test]
    fn test_value_to_json_array() {
        let value = Value::Array(vec![Value::Integer(1), Value::Integer(2), Value::Integer(3)]);
        let json = value_to_json(&value);
        assert_eq!(json, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_value_to_json_dictionary() {
        let mut dict = IndexMap::new();
        dict.insert("key".to_string(), Value::String("value".to_string()));
        let value = Value::Dictionary(dict);
        let json = value_to_json(&value);
        assert_eq!(json, serde_json::json!({"key": "value"}));
    }

    #[test]
    fn test_builtin_jmespath_simple_key() {
        let mut dict = IndexMap::new();
        dict.insert("name".to_string(), Value::String("John".to_string()));
        dict.insert("age".to_string(), Value::Integer(30));
        let data = Value::Dictionary(dict);

        let result = builtin_jmespath(vec![data, Value::String("name".to_string())]).unwrap();
        assert_eq!(result, Value::String("John".to_string()));
    }

    #[test]
    fn test_builtin_jmespath_nested_key() {
        let mut inner_dict = IndexMap::new();
        inner_dict.insert("bar".to_string(), Value::String("baz".to_string()));

        let mut outer_dict = IndexMap::new();
        outer_dict.insert("foo".to_string(), Value::Dictionary(inner_dict));

        let data = Value::Dictionary(outer_dict);

        let result = builtin_jmespath(vec![data, Value::String("foo.bar".to_string())]).unwrap();
        assert_eq!(result, Value::String("baz".to_string()));
    }

    #[test]
    fn test_builtin_jmespath_array_index() {
        let data = Value::Array(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]);

        let result = builtin_jmespath(vec![data, Value::String("[1]".to_string())]).unwrap();
        assert_eq!(result, Value::Integer(2));
    }

    #[test]
    fn test_builtin_jmespath_projection() {
        let mut dict1 = IndexMap::new();
        dict1.insert("name".to_string(), Value::String("John".to_string()));

        let mut dict2 = IndexMap::new();
        dict2.insert("name".to_string(), Value::String("Jane".to_string()));

        let data = Value::Array(vec![Value::Dictionary(dict1), Value::Dictionary(dict2)]);

        let result = builtin_jmespath(vec![data, Value::String("[*].name".to_string())]).unwrap();
        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0], Value::String("John".to_string()));
            assert_eq!(arr[1], Value::String("Jane".to_string()));
        } else {
            panic!("Expected array result");
        }
    }

    #[test]
    fn test_builtin_jmespath_embedded_key() {
        let mut inner = IndexMap::new();
        inner.insert("Key".to_string(), Value::String("value".to_string()));

        let mut outer = IndexMap::new();
        outer.insert("embedded".to_string(), Value::Dictionary(inner));

        let data = Value::Dictionary(outer);

        let result =
            builtin_jmespath(vec![data, Value::String("embedded.Key".to_string())]).unwrap();
        assert_eq!(result, Value::String("value".to_string()));
    }

    #[test]
    fn test_builtin_jmespath_nonexistent_key() {
        let mut dict = IndexMap::new();
        dict.insert("key".to_string(), Value::String("value".to_string()));
        let data = Value::Dictionary(dict);

        let result =
            builtin_jmespath(vec![data, Value::String("nonexistent".to_string())]).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_builtin_jmespath_wrong_arg_count() {
        let result = builtin_jmespath(vec![Value::Null]);
        assert!(matches!(
            result,
            Err(RuntimeError::WrongArgumentCount { .. })
        ));
    }

    #[test]
    fn test_builtin_jmespath_invalid_expression() {
        let data = Value::Dictionary(IndexMap::new());
        let result = builtin_jmespath(vec![data, Value::String("[[invalid".to_string())]);
        assert!(matches!(result, Err(RuntimeError::InvalidArgument { .. })));
    }

    #[test]
    fn test_builtin_jmespath_non_string_expression() {
        let data = Value::Dictionary(IndexMap::new());
        let result = builtin_jmespath(vec![data, Value::Integer(42)]);
        assert!(matches!(result, Err(RuntimeError::InvalidArgument { .. })));
    }
}
