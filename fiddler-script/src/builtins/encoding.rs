//! Encoding built-in functions (base64).

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};

use crate::error::RuntimeError;
use crate::Value;

/// Encode bytes to base64.
pub fn builtin_base64_encode(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    let input = args[0].to_bytes();
    let encoded = BASE64_STANDARD.encode(&input);
    Ok(Value::String(encoded))
}

/// Decode base64 to bytes.
pub fn builtin_base64_decode(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    let input = match &args[0] {
        Value::String(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => {
            return Err(RuntimeError::invalid_argument(
                "base64_decode() requires a string or bytes argument".to_string(),
            ))
        }
    };

    let decoded = BASE64_STANDARD
        .decode(&input)
        .map_err(|e| RuntimeError::invalid_argument(format!("base64 decode failed: {}", e)))?;

    Ok(Value::Bytes(decoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_roundtrip() {
        let input = Value::String("hello world".to_string());
        let encoded = builtin_base64_encode(vec![input]).unwrap();
        assert_eq!(encoded, Value::String("aGVsbG8gd29ybGQ=".to_string()));

        let decoded = builtin_base64_decode(vec![encoded]).unwrap();
        assert_eq!(decoded, Value::Bytes(b"hello world".to_vec()));
    }

    #[test]
    fn test_base64_decode_invalid() {
        let result = builtin_base64_decode(vec![Value::String("not valid base64!!!".to_string())]);
        assert!(matches!(result, Err(RuntimeError::InvalidArgument { .. })));
    }
}
