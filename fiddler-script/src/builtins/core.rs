//! Core built-in functions.

use crate::error::RuntimeError;
use crate::Value;

/// Print values to stdout.
///
/// # Arguments
/// - Any number of values to print
///
/// # Returns
/// - `Value::Null`
pub fn builtin_print(args: Vec<Value>) -> Result<Value, RuntimeError> {
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
pub fn builtin_len(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    match &args[0] {
        Value::String(s) => Ok(Value::Integer(s.len() as i64)),
        Value::Bytes(b) => Ok(Value::Integer(b.len() as i64)),
        Value::Array(a) => Ok(Value::Integer(a.len() as i64)),
        Value::Dictionary(d) => Ok(Value::Integer(d.len() as i64)),
        _ => Err(RuntimeError::invalid_argument(
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
pub fn builtin_str(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    Ok(Value::String(args[0].to_string()))
}

/// Convert a value to an integer.
///
/// # Arguments
/// - A string, integer, float, boolean, bytes, array, dictionary, or null value
///
/// # Returns
/// - The value as an integer (floats are truncated)
pub fn builtin_int(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    match &args[0] {
        Value::String(s) => s.parse::<i64>().map(Value::Integer).map_err(|_| {
            RuntimeError::invalid_argument(format!("Cannot convert '{}' to integer", s))
        }),
        Value::Integer(n) => Ok(Value::Integer(*n)),
        Value::Float(f) => Ok(Value::Integer(f.trunc() as i64)),
        Value::Boolean(b) => Ok(Value::Integer(if *b { 1 } else { 0 })),
        Value::Bytes(bytes) => {
            let s = String::from_utf8_lossy(bytes);
            s.parse::<i64>().map(Value::Integer).map_err(|_| {
                RuntimeError::invalid_argument("Cannot convert bytes to integer".to_string())
            })
        }
        Value::Array(a) => Ok(Value::Integer(a.len() as i64)),
        Value::Dictionary(d) => Ok(Value::Integer(d.len() as i64)),
        Value::Null => Ok(Value::Integer(0)),
    }
}

/// Convert a value to a float.
///
/// # Arguments
/// - A numeric, string, or boolean value
///
/// # Returns
/// - The value as a float (f64)
///
/// # Examples
/// ```ignore
/// float(42);         // 42.0
/// float("3.14");     // 3.14
/// float(true);       // 1.0
/// float(false);      // 0.0
/// ```
pub fn builtin_float(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    match &args[0] {
        Value::Float(f) => Ok(Value::Float(*f)),
        Value::Integer(n) => Ok(Value::Float(*n as f64)),
        Value::Boolean(b) => Ok(Value::Float(if *b { 1.0 } else { 0.0 })),
        Value::String(s) => s.parse::<f64>().map(Value::Float).map_err(|_| {
            RuntimeError::invalid_argument(format!("Cannot convert '{}' to float", s))
        }),
        Value::Bytes(bytes) => {
            let s = String::from_utf8_lossy(bytes);
            s.parse::<f64>().map(Value::Float).map_err(|_| {
                RuntimeError::invalid_argument("Cannot convert bytes to float".to_string())
            })
        }
        _ => Err(RuntimeError::invalid_argument(
            "Cannot convert value to float".to_string(),
        )),
    }
}

/// Get an environment variable by name.
///
/// # Arguments
/// - The name of the environment variable
///
/// # Returns
/// - The value as a string, or null if not found
pub fn builtin_getenv(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    let name = match &args[0] {
        Value::String(s) => s.clone(),
        _ => {
            return Err(RuntimeError::invalid_argument(
                "getenv() requires a string argument".to_string(),
            ))
        }
    };

    match std::env::var(&name) {
        Ok(value) => Ok(Value::String(value)),
        Err(_) => Ok(Value::Null),
    }
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
        assert!(matches!(result, Err(RuntimeError::InvalidArgument { .. })));
    }

    #[test]
    fn test_builtin_len_wrong_count() {
        let result = builtin_len(vec![]);
        assert!(matches!(
            result,
            Err(RuntimeError::WrongArgumentCount {
                expected: 1,
                actual: 0,
                ..
            })
        ));
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
        assert!(matches!(result, Err(RuntimeError::InvalidArgument { .. })));
    }

    #[test]
    fn test_builtin_getenv() {
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
}
