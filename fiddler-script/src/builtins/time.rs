//! Time and timestamp built-in functions.

use crate::error::RuntimeError;
use crate::Value;
use chrono::Utc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Get the current Unix timestamp in seconds.
///
/// # Returns
/// - The number of seconds since the Unix epoch (January 1, 1970 00:00:00 UTC)
///
/// # Examples
/// ```ignore
/// let now = timestamp();       // e.g., 1706284800
/// print("Current time:", now);
/// ```
pub fn builtin_timestamp(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if !args.is_empty() {
        return Err(RuntimeError::wrong_argument_count(0, args.len()));
    }

    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => Ok(Value::Integer(duration.as_secs() as i64)),
        Err(_) => Err(RuntimeError::invalid_argument(
            "System time is before Unix epoch".to_string(),
        )),
    }
}

/// Get the current Unix timestamp in seconds (alias for timestamp).
///
/// # Returns
/// - The number of seconds since the Unix epoch
///
/// # Examples
/// ```ignore
/// let now = epoch();           // e.g., 1706284800
/// ```
pub fn builtin_epoch(args: Vec<Value>) -> Result<Value, RuntimeError> {
    builtin_timestamp(args)
}

/// Get the current Unix timestamp in milliseconds.
///
/// # Returns
/// - The number of milliseconds since the Unix epoch
///
/// # Examples
/// ```ignore
/// let now_ms = timestamp_millis();  // e.g., 1706284800123
/// ```
pub fn builtin_timestamp_millis(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if !args.is_empty() {
        return Err(RuntimeError::wrong_argument_count(0, args.len()));
    }

    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => Ok(Value::Integer(duration.as_millis() as i64)),
        Err(_) => Err(RuntimeError::invalid_argument(
            "System time is before Unix epoch".to_string(),
        )),
    }
}

/// Get the current Unix timestamp in microseconds.
///
/// # Returns
/// - The number of microseconds since the Unix epoch
///
/// # Examples
/// ```ignore
/// let now_us = timestamp_micros();  // e.g., 1706284800123456
/// ```
pub fn builtin_timestamp_micros(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if !args.is_empty() {
        return Err(RuntimeError::wrong_argument_count(0, args.len()));
    }

    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => Ok(Value::Integer(duration.as_micros() as i64)),
        Err(_) => Err(RuntimeError::invalid_argument(
            "System time is before Unix epoch".to_string(),
        )),
    }
}

/// Get the current time as an ISO 8601 formatted string.
///
/// # Returns
/// - A string in ISO 8601 format (RFC 3339): "YYYY-MM-DDTHH:MM:SS.sssZ"
///
/// # Examples
/// ```ignore
/// let now = timestamp_iso8601();  // e.g., "2024-01-26T12:34:56.789123456+00:00"
/// print("Current time:", now);
/// ```
pub fn builtin_timestamp_iso8601(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if !args.is_empty() {
        return Err(RuntimeError::wrong_argument_count(0, args.len()));
    }

    let now = Utc::now();
    let iso_string = now.to_rfc3339();

    Ok(Value::String(iso_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp() {
        let result = builtin_timestamp(vec![]).unwrap();
        if let Value::Integer(ts) = result {
            // Should be a reasonable timestamp (after year 2020)
            assert!(ts > 1577836800); // Jan 1, 2020
        } else {
            panic!("Expected integer timestamp");
        }
    }

    #[test]
    fn test_epoch_alias() {
        let result = builtin_epoch(vec![]).unwrap();
        assert!(matches!(result, Value::Integer(_)));
    }

    #[test]
    fn test_timestamp_millis() {
        let result = builtin_timestamp_millis(vec![]).unwrap();
        if let Value::Integer(ts) = result {
            // Milliseconds should be larger than seconds
            assert!(ts > 1577836800000); // Jan 1, 2020 in ms
        } else {
            panic!("Expected integer timestamp");
        }
    }

    #[test]
    fn test_timestamp_micros() {
        let result = builtin_timestamp_micros(vec![]).unwrap();
        if let Value::Integer(ts) = result {
            // Microseconds should be even larger
            assert!(ts > 1577836800000000); // Jan 1, 2020 in us
        } else {
            panic!("Expected integer timestamp");
        }
    }

    #[test]
    fn test_timestamp_iso8601() {
        let result = builtin_timestamp_iso8601(vec![]).unwrap();
        if let Value::String(s) = result {
            // Should contain basic ISO 8601 markers
            assert!(s.contains('T'));
            assert!(s.contains('-'));
            assert!(s.contains(':'));
        } else {
            panic!("Expected string timestamp");
        }
    }

    #[test]
    fn test_timestamp_wrong_arg_count() {
        let result = builtin_timestamp(vec![Value::Integer(42)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_timestamp_consistency() {
        // All timestamp functions should return values from approximately the same time
        let ts_sec = builtin_timestamp(vec![]).unwrap();
        let ts_ms = builtin_timestamp_millis(vec![]).unwrap();

        if let (Value::Integer(sec), Value::Integer(ms)) = (ts_sec, ts_ms) {
            let sec_from_ms = ms / 1000;
            // Should be within 1 second of each other
            assert!((sec - sec_from_ms).abs() <= 1);
        }
    }
}
