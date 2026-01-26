//! Built-in functions for FiddlerScript.
//!
//! This module provides the default built-in functions and a system for
//! registering custom built-in functions.

use std::collections::HashMap;

use crate::error::RuntimeError;
use crate::Value;

/// Type alias for built-in function signatures.
pub type BuiltinFn = fn(Vec<Value>) -> Result<Value, RuntimeError>;

/// Get the default built-in functions.
pub fn get_default_builtins() -> HashMap<String, BuiltinFn> {
    let mut builtins: HashMap<String, BuiltinFn> = HashMap::new();
    builtins.insert("print".to_string(), builtin_print);
    builtins.insert("len".to_string(), builtin_len);
    builtins.insert("str".to_string(), builtin_str);
    builtins.insert("int".to_string(), builtin_int);
    builtins.insert("getenv".to_string(), builtin_getenv);
    builtins.insert("parse_json".to_string(), builtin_parse_json);
    builtins.insert("bytes_to_string".to_string(), builtin_bytes_to_string);
    builtins.insert("bytes".to_string(), builtin_bytes);
    builtins.insert("array".to_string(), builtin_array);
    builtins.insert("push".to_string(), builtin_push);
    builtins.insert("get".to_string(), builtin_get);
    builtins.insert("set".to_string(), builtin_set);
    builtins.insert("dict".to_string(), builtin_dict);
    builtins.insert("keys".to_string(), builtin_keys);
    builtins.insert("is_array".to_string(), builtin_is_array);
    builtins.insert("is_dict".to_string(), builtin_is_dict);
    builtins
}

/// Print values to stdout.
///
/// # Arguments
/// - Any number of values to print
///
/// # Returns
/// - `Value::Null`
fn builtin_print(args: Vec<Value>) -> Result<Value, RuntimeError> {
    let output: Vec<String> = args.iter().map(|v| v.to_string()).collect();
    println!("{}", output.join(" "));
    Ok(Value::Null)
}

/// Get the length of a string, bytes, array, or dictionary.
///
/// # Arguments
/// - A string, bytes, array, or dictionary value
///
/// # Returns
/// - The length as an integer
fn builtin_len(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    match &args[0] {
        Value::String(s) => Ok(Value::Integer(s.len() as i64)),
        Value::Bytes(b) => Ok(Value::Integer(b.len() as i64)),
        Value::Array(a) => Ok(Value::Integer(a.len() as i64)),
        Value::Dictionary(d) => Ok(Value::Integer(d.len() as i64)),
        _ => Err(RuntimeError::InvalidArgument(
            "len() requires a string, bytes, array, or dictionary argument".to_string(),
        )),
    }
}

/// Convert a value to a string.
///
/// # Arguments
/// - Any value
///
/// # Returns
/// - The value as a string
fn builtin_str(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    Ok(Value::String(args[0].to_string()))
}

/// Convert a string to an integer.
///
/// # Arguments
/// - A string containing a valid integer
///
/// # Returns
/// - The parsed integer value
fn builtin_int(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    match &args[0] {
        Value::String(s) => s.parse::<i64>().map(Value::Integer).map_err(|_| {
            RuntimeError::InvalidArgument(format!("Cannot convert '{}' to integer", s))
        }),
        Value::Integer(n) => Ok(Value::Integer(*n)),
        Value::Boolean(b) => Ok(Value::Integer(if *b { 1 } else { 0 })),
        Value::Bytes(bytes) => {
            let s = String::from_utf8_lossy(bytes);
            s.parse::<i64>().map(Value::Integer).map_err(|_| {
                RuntimeError::InvalidArgument("Cannot convert bytes to integer".to_string())
            })
        }
        Value::Array(a) => Ok(Value::Integer(a.len() as i64)),
        Value::Dictionary(d) => Ok(Value::Integer(d.len() as i64)),
        Value::Null => Ok(Value::Integer(0)),
    }
}

/// Get an environment variable by name.
///
/// # Arguments
/// - The name of the environment variable
///
/// # Returns
/// - The value as a string, or null if not found
fn builtin_getenv(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let name = match &args[0] {
        Value::String(s) => s.clone(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "getenv() requires a string argument".to_string(),
            ))
        }
    };

    match std::env::var(&name) {
        Ok(value) => Ok(Value::String(value)),
        Err(_) => Ok(Value::Null),
    }
}

/// Parse JSON from bytes.
///
/// # Arguments
/// - Bytes containing valid JSON
///
/// # Returns
/// - Parsed value (string, integer, boolean, or null)
fn builtin_parse_json(args: Vec<Value>) -> Result<Value, RuntimeError> {
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

    let json_value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        RuntimeError::InvalidArgument(format!("Invalid JSON: {}", e))
    })?;

    Ok(json_to_value(json_value))
}

/// Convert a serde_json::Value to a FiddlerScript Value.
fn json_to_value(json: serde_json::Value) -> Value {
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
        serde_json::Value::Array(arr) => {
            Value::Array(arr.into_iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(obj) => {
            let dict: std::collections::HashMap<String, Value> = obj
                .into_iter()
                .map(|(k, v)| (k, json_to_value(v)))
                .collect();
            Value::Dictionary(dict)
        }
    }
}

/// Convert bytes to a string.
///
/// # Arguments
/// - Bytes to convert
///
/// # Returns
/// - The bytes interpreted as a UTF-8 string
fn builtin_bytes_to_string(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    match &args[0] {
        Value::Bytes(bytes) => {
            let s = String::from_utf8_lossy(bytes).into_owned();
            Ok(Value::String(s))
        }
        Value::String(s) => Ok(Value::String(s.clone())),
        _ => Err(RuntimeError::InvalidArgument(
            "bytes_to_string() requires bytes or string argument".to_string(),
        )),
    }
}

/// Convert a value to bytes.
///
/// # Arguments
/// - Any value to convert to bytes
///
/// # Returns
/// - The value as bytes
fn builtin_bytes(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    Ok(Value::Bytes(args[0].to_bytes()))
}

/// Create a new array from arguments.
///
/// # Arguments
/// - Any number of values to include in the array
///
/// # Returns
/// - An array containing all arguments
fn builtin_array(args: Vec<Value>) -> Result<Value, RuntimeError> {
    Ok(Value::Array(args))
}

/// Push a value onto an array.
///
/// # Arguments
/// - An array
/// - A value to push
///
/// # Returns
/// - A new array with the value appended
fn builtin_push(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::WrongArgumentCount(2, args.len()));
    }

    match &args[0] {
        Value::Array(arr) => {
            let mut new_arr = arr.clone();
            new_arr.push(args[1].clone());
            Ok(Value::Array(new_arr))
        }
        _ => Err(RuntimeError::InvalidArgument(
            "push() requires an array as first argument".to_string(),
        )),
    }
}

/// Get a value from an array, dictionary, or string.
///
/// # Arguments
/// - An array, dictionary, or string
/// - An index (integer for array/string) or key (string for dictionary)
///
/// # Returns
/// - The value at the index/key, or null if not found
fn builtin_get(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::WrongArgumentCount(2, args.len()));
    }

    match (&args[0], &args[1]) {
        (Value::Array(arr), Value::Integer(idx)) => {
            let idx = *idx as usize;
            Ok(arr.get(idx).cloned().unwrap_or(Value::Null))
        }
        (Value::String(s), Value::Integer(idx)) => {
            let idx = *idx as usize;
            Ok(s.chars()
                .nth(idx)
                .map(|c| Value::String(c.to_string()))
                .unwrap_or(Value::Null))
        }
        (Value::Dictionary(dict), Value::String(key)) => {
            Ok(dict.get(key).cloned().unwrap_or(Value::Null))
        }
        (Value::Array(_) | Value::String(_), _) => Err(RuntimeError::InvalidArgument(
            "Array/string index must be an integer".to_string(),
        )),
        (Value::Dictionary(_), _) => Err(RuntimeError::InvalidArgument(
            "Dictionary key must be a string".to_string(),
        )),
        _ => Err(RuntimeError::InvalidArgument(
            "get() requires an array, dictionary, or string as first argument".to_string(),
        )),
    }
}

/// Set a value in an array or dictionary.
///
/// # Arguments
/// - An array or dictionary
/// - An index (integer for array) or key (string for dictionary)
/// - The value to set
///
/// # Returns
/// - A new array/dictionary with the value set
fn builtin_set(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 3 {
        return Err(RuntimeError::WrongArgumentCount(3, args.len()));
    }

    match (&args[0], &args[1]) {
        (Value::Array(arr), Value::Integer(idx)) => {
            let idx = *idx as usize;
            let mut new_arr = arr.clone();
            if idx < new_arr.len() {
                new_arr[idx] = args[2].clone();
            } else {
                // Extend array if needed
                while new_arr.len() <= idx {
                    new_arr.push(Value::Null);
                }
                new_arr[idx] = args[2].clone();
            }
            Ok(Value::Array(new_arr))
        }
        (Value::Dictionary(dict), Value::String(key)) => {
            let mut new_dict = dict.clone();
            new_dict.insert(key.clone(), args[2].clone());
            Ok(Value::Dictionary(new_dict))
        }
        (Value::Array(_), _) => Err(RuntimeError::InvalidArgument(
            "Array index must be an integer".to_string(),
        )),
        (Value::Dictionary(_), _) => Err(RuntimeError::InvalidArgument(
            "Dictionary key must be a string".to_string(),
        )),
        _ => Err(RuntimeError::InvalidArgument(
            "set() requires an array or dictionary as first argument".to_string(),
        )),
    }
}

/// Create an empty dictionary.
///
/// # Returns
/// - An empty dictionary
fn builtin_dict(_args: Vec<Value>) -> Result<Value, RuntimeError> {
    Ok(Value::Dictionary(std::collections::HashMap::new()))
}

/// Get all keys from a dictionary.
///
/// # Arguments
/// - A dictionary
///
/// # Returns
/// - An array of keys
fn builtin_keys(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    match &args[0] {
        Value::Dictionary(dict) => {
            let keys: Vec<Value> = dict.keys().map(|k| Value::String(k.clone())).collect();
            Ok(Value::Array(keys))
        }
        _ => Err(RuntimeError::InvalidArgument(
            "keys() requires a dictionary argument".to_string(),
        )),
    }
}

/// Check if a value is an array.
///
/// # Arguments
/// - Any value
///
/// # Returns
/// - true if the value is an array, false otherwise
fn builtin_is_array(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    Ok(Value::Boolean(args[0].is_array()))
}

/// Check if a value is a dictionary.
///
/// # Arguments
/// - Any value
///
/// # Returns
/// - true if the value is a dictionary, false otherwise
fn builtin_is_dict(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    Ok(Value::Boolean(args[0].is_dictionary()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_len() {
        let result = builtin_len(vec![Value::String("hello".to_string())]).unwrap();
        assert_eq!(result, Value::Integer(5));
    }

    #[test]
    fn test_builtin_len_empty() {
        let result = builtin_len(vec![Value::String("".to_string())]).unwrap();
        assert_eq!(result, Value::Integer(0));
    }

    #[test]
    fn test_builtin_len_wrong_type() {
        let result = builtin_len(vec![Value::Integer(42)]);
        assert!(matches!(result, Err(RuntimeError::InvalidArgument(_))));
    }

    #[test]
    fn test_builtin_len_wrong_count() {
        let result = builtin_len(vec![]);
        assert!(matches!(result, Err(RuntimeError::WrongArgumentCount(1, 0))));
    }

    #[test]
    fn test_builtin_str() {
        assert_eq!(
            builtin_str(vec![Value::Integer(42)]).unwrap(),
            Value::String("42".to_string())
        );
        assert_eq!(
            builtin_str(vec![Value::Boolean(true)]).unwrap(),
            Value::String("true".to_string())
        );
        assert_eq!(
            builtin_str(vec![Value::String("hello".to_string())]).unwrap(),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn test_builtin_int() {
        assert_eq!(
            builtin_int(vec![Value::String("42".to_string())]).unwrap(),
            Value::Integer(42)
        );
        assert_eq!(
            builtin_int(vec![Value::Boolean(true)]).unwrap(),
            Value::Integer(1)
        );
        assert_eq!(
            builtin_int(vec![Value::Boolean(false)]).unwrap(),
            Value::Integer(0)
        );
    }

    #[test]
    fn test_builtin_int_invalid() {
        let result = builtin_int(vec![Value::String("not a number".to_string())]);
        assert!(matches!(result, Err(RuntimeError::InvalidArgument(_))));
    }

    #[test]
    fn test_default_builtins() {
        let builtins = get_default_builtins();
        assert!(builtins.contains_key("print"));
        assert!(builtins.contains_key("len"));
        assert!(builtins.contains_key("str"));
        assert!(builtins.contains_key("int"));
        assert!(builtins.contains_key("getenv"));
        assert!(builtins.contains_key("parse_json"));
        assert!(builtins.contains_key("bytes_to_string"));
        assert!(builtins.contains_key("bytes"));
    }

    #[test]
    fn test_builtin_getenv() {
        // Set a test environment variable
        std::env::set_var("FIDDLER_TEST_VAR", "test_value");
        let result = builtin_getenv(vec![Value::String("FIDDLER_TEST_VAR".to_string())]).unwrap();
        assert_eq!(result, Value::String("test_value".to_string()));
        std::env::remove_var("FIDDLER_TEST_VAR");
    }

    #[test]
    fn test_builtin_getenv_not_found() {
        let result =
            builtin_getenv(vec![Value::String("NONEXISTENT_VAR_12345".to_string())]).unwrap();
        assert_eq!(result, Value::Null);
    }

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
        // Objects are converted to Dictionary
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

    #[test]
    fn test_builtin_bytes_to_string() {
        let bytes = "hello".as_bytes().to_vec();
        let result = builtin_bytes_to_string(vec![Value::Bytes(bytes)]).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_builtin_bytes_to_string_from_string() {
        let result = builtin_bytes_to_string(vec![Value::String("hello".to_string())]).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_builtin_bytes() {
        let result = builtin_bytes(vec![Value::String("hello".to_string())]).unwrap();
        assert_eq!(result, Value::Bytes("hello".as_bytes().to_vec()));
    }

    #[test]
    fn test_builtin_bytes_from_int() {
        let result = builtin_bytes(vec![Value::Integer(42)]).unwrap();
        assert_eq!(result, Value::Bytes("42".as_bytes().to_vec()));
    }
}
