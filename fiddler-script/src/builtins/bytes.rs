//! Bytes conversion built-in functions.

use crate::error::RuntimeError;
use crate::Value;

/// Convert bytes to a string.
///
/// # Arguments
/// - Bytes to convert
///
/// # Returns
/// - The bytes interpreted as a UTF-8 string
pub fn builtin_bytes_to_string(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    match &args[0] {
        Value::Bytes(bytes) => {
            let s = String::from_utf8_lossy(bytes).into_owned();
            Ok(Value::String(s))
        }
        Value::String(s) => Ok(Value::String(s.clone())),
        _ => Err(RuntimeError::invalid_argument(
            "bytes_to_string() requires bytes or string argument",
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
pub fn builtin_bytes(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    Ok(Value::Bytes(args[0].to_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
