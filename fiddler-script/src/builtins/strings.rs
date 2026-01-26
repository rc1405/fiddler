//! String manipulation built-in functions.

use crate::error::RuntimeError;
use crate::Value;

/// Split bytes/string into lines.
pub fn builtin_lines(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let input = match &args[0] {
        Value::String(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "lines() requires a string or bytes argument".to_string(),
            ))
        }
    };

    let lines: Vec<Value> = input
        .split('\n')
        .map(|line| Value::String(line.to_string()))
        .collect();

    Ok(Value::Array(lines))
}

/// Capitalize the first character of a string.
pub fn builtin_capitalize(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let s = match &args[0] {
        Value::String(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "capitalize() requires a string argument".to_string(),
            ))
        }
    };

    let mut chars = s.chars();
    let result = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    };

    Ok(Value::String(result))
}

/// Convert a string to lowercase.
pub fn builtin_lowercase(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let s = match &args[0] {
        Value::String(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "lowercase() requires a string argument".to_string(),
            ))
        }
    };

    Ok(Value::String(s.to_lowercase()))
}

/// Convert a string to uppercase.
pub fn builtin_uppercase(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let s = match &args[0] {
        Value::String(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "uppercase() requires a string argument".to_string(),
            ))
        }
    };

    Ok(Value::String(s.to_uppercase()))
}

/// Trim whitespace from both ends of a string.
pub fn builtin_trim(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let s = match &args[0] {
        Value::String(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "trim() requires a string argument".to_string(),
            ))
        }
    };

    Ok(Value::String(s.trim().to_string()))
}

/// Remove a prefix from a string.
pub fn builtin_trim_prefix(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::WrongArgumentCount(2, args.len()));
    }

    let s = match &args[0] {
        Value::String(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "trim_prefix() requires a string as first argument".to_string(),
            ))
        }
    };

    let prefix = match &args[1] {
        Value::String(s) => s.clone(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "trim_prefix() requires a string as second argument".to_string(),
            ))
        }
    };

    let result = s.strip_prefix(&prefix).unwrap_or(&s).to_string();
    Ok(Value::String(result))
}

/// Remove a suffix from a string.
pub fn builtin_trim_suffix(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::WrongArgumentCount(2, args.len()));
    }

    let s = match &args[0] {
        Value::String(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "trim_suffix() requires a string as first argument".to_string(),
            ))
        }
    };

    let suffix = match &args[1] {
        Value::String(s) => s.clone(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "trim_suffix() requires a string as second argument".to_string(),
            ))
        }
    };

    let result = s.strip_suffix(&suffix).unwrap_or(&s).to_string();
    Ok(Value::String(result))
}

/// Check if a string starts with a prefix.
pub fn builtin_has_prefix(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::WrongArgumentCount(2, args.len()));
    }

    let s = match &args[0] {
        Value::String(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "has_prefix() requires a string as first argument".to_string(),
            ))
        }
    };

    let prefix = match &args[1] {
        Value::String(s) => s.clone(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "has_prefix() requires a string as second argument".to_string(),
            ))
        }
    };

    Ok(Value::Boolean(s.starts_with(&prefix)))
}

/// Check if a string ends with a suffix.
pub fn builtin_has_suffix(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::WrongArgumentCount(2, args.len()));
    }

    let s = match &args[0] {
        Value::String(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "has_suffix() requires a string as first argument".to_string(),
            ))
        }
    };

    let suffix = match &args[1] {
        Value::String(s) => s.clone(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "has_suffix() requires a string as second argument".to_string(),
            ))
        }
    };

    Ok(Value::Boolean(s.ends_with(&suffix)))
}

/// Split a string by a delimiter.
pub fn builtin_split(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::WrongArgumentCount(2, args.len()));
    }

    let s = match &args[0] {
        Value::String(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "split() requires a string as first argument".to_string(),
            ))
        }
    };

    let delimiter = match &args[1] {
        Value::String(s) => s.clone(),
        _ => {
            return Err(RuntimeError::InvalidArgument(
                "split() requires a string as second argument".to_string(),
            ))
        }
    };

    let parts: Vec<Value> = s
        .split(&delimiter)
        .map(|part| Value::String(part.to_string()))
        .collect();

    Ok(Value::Array(parts))
}

/// Reverse a string, array, or bytes.
pub fn builtin_reverse(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    match &args[0] {
        Value::String(s) => {
            let reversed: String = s.chars().rev().collect();
            Ok(Value::String(reversed))
        }
        Value::Array(arr) => {
            let reversed: Vec<Value> = arr.iter().rev().cloned().collect();
            Ok(Value::Array(reversed))
        }
        Value::Bytes(b) => {
            let reversed: Vec<u8> = b.iter().rev().cloned().collect();
            Ok(Value::Bytes(reversed))
        }
        _ => Err(RuntimeError::InvalidArgument(
            "reverse() requires a string, array, or bytes argument".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lines() {
        let input = Value::String("a\nb\nc".to_string());
        let result = builtin_lines(vec![input]).unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
                Value::String("c".to_string()),
            ])
        );
    }

    #[test]
    fn test_capitalize() {
        let result = builtin_capitalize(vec![Value::String("hello".to_string())]).unwrap();
        assert_eq!(result, Value::String("Hello".to_string()));
    }

    #[test]
    fn test_lowercase() {
        let result = builtin_lowercase(vec![Value::String("HELLO".to_string())]).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_uppercase() {
        let result = builtin_uppercase(vec![Value::String("hello".to_string())]).unwrap();
        assert_eq!(result, Value::String("HELLO".to_string()));
    }

    #[test]
    fn test_trim() {
        let result = builtin_trim(vec![Value::String("  hello  ".to_string())]).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_trim_prefix() {
        let result = builtin_trim_prefix(vec![
            Value::String("hello world".to_string()),
            Value::String("hello ".to_string()),
        ])
        .unwrap();
        assert_eq!(result, Value::String("world".to_string()));
    }

    #[test]
    fn test_trim_suffix() {
        let result = builtin_trim_suffix(vec![
            Value::String("hello.txt".to_string()),
            Value::String(".txt".to_string()),
        ])
        .unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_has_prefix() {
        let result = builtin_has_prefix(vec![
            Value::String("hello world".to_string()),
            Value::String("hello".to_string()),
        ])
        .unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    #[test]
    fn test_has_suffix() {
        let result = builtin_has_suffix(vec![
            Value::String("hello.txt".to_string()),
            Value::String(".txt".to_string()),
        ])
        .unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    #[test]
    fn test_split() {
        let result = builtin_split(vec![
            Value::String("a,b,c".to_string()),
            Value::String(",".to_string()),
        ])
        .unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
                Value::String("c".to_string()),
            ])
        );
    }

    #[test]
    fn test_reverse_string() {
        let result = builtin_reverse(vec![Value::String("hello".to_string())]).unwrap();
        assert_eq!(result, Value::String("olleh".to_string()));
    }

    #[test]
    fn test_reverse_array() {
        let result = builtin_reverse(vec![Value::Array(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ])])
        .unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::Integer(3),
                Value::Integer(2),
                Value::Integer(1),
            ])
        );
    }
}
