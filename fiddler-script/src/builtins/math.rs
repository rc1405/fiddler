//! Math built-in functions.

use crate::error::RuntimeError;
use crate::Value;

/// Get the absolute value of a number.
///
/// # Arguments
/// - An integer or float value
///
/// # Returns
/// - The absolute value (same type as input)
///
/// # Examples
/// ```ignore
/// abs(-42);      // 42
/// abs(-3.14);    // 3.14
/// let x = -10;
/// x.abs();       // 10 (method syntax)
/// ```
pub fn builtin_abs(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(n.saturating_abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        _ => Err(RuntimeError::invalid_argument(
            "abs() requires a numeric argument".to_string(),
        )),
    }
}

/// Ceiling function - rounds up to nearest integer.
///
/// # Arguments
/// - An integer or float value
///
/// # Returns
/// - For integers: the same integer
/// - For floats: the smallest integer greater than or equal to the value
///
/// # Examples
/// ```ignore
/// ceil(42);      // 42
/// ceil(3.14);    // 4
/// ceil(-3.14);   // -3
/// let x = 2.5;
/// x.ceil();      // 3 (method syntax)
/// ```
pub fn builtin_ceil(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(*n)),
        Value::Float(f) => Ok(Value::Integer(f.ceil() as i64)),
        _ => Err(RuntimeError::invalid_argument(
            "ceil() requires a numeric argument".to_string(),
        )),
    }
}

/// Floor function - rounds down to nearest integer.
///
/// # Arguments
/// - An integer or float value
///
/// # Returns
/// - For integers: the same integer
/// - For floats: the largest integer less than or equal to the value
///
/// # Examples
/// ```ignore
/// floor(42);     // 42
/// floor(3.14);   // 3
/// floor(-3.14);  // -4
/// let x = 2.5;
/// x.floor();     // 2 (method syntax)
/// ```
pub fn builtin_floor(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(*n)),
        Value::Float(f) => Ok(Value::Integer(f.floor() as i64)),
        _ => Err(RuntimeError::invalid_argument(
            "floor() requires a numeric argument".to_string(),
        )),
    }
}

/// Round function - rounds to nearest integer.
///
/// # Arguments
/// - An integer or float value
///
/// # Returns
/// - For integers: the same integer
/// - For floats: the nearest integer (rounds half away from zero)
///
/// # Examples
/// ```ignore
/// round(42);     // 42
/// round(3.4);    // 3
/// round(3.5);    // 4
/// round(-3.5);   // -4
/// let x = 2.5;
/// x.round();     // 3 (method syntax)
/// ```
pub fn builtin_round(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(*n)),
        Value::Float(f) => Ok(Value::Integer(f.round() as i64)),
        _ => Err(RuntimeError::invalid_argument(
            "round() requires a numeric argument".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abs_positive() {
        let result = builtin_abs(vec![Value::Integer(42)]).unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_abs_negative() {
        let result = builtin_abs(vec![Value::Integer(-42)]).unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_abs_zero() {
        let result = builtin_abs(vec![Value::Integer(0)]).unwrap();
        assert_eq!(result, Value::Integer(0));
    }

    #[test]
    fn test_abs_min_value() {
        // saturating_abs handles i64::MIN correctly
        let result = builtin_abs(vec![Value::Integer(i64::MIN)]).unwrap();
        assert_eq!(result, Value::Integer(i64::MAX));
    }

    #[test]
    fn test_abs_float_positive() {
        let result = builtin_abs(vec![Value::Float(3.14)]).unwrap();
        assert_eq!(result, Value::Float(3.14));
    }

    #[test]
    fn test_abs_float_negative() {
        let result = builtin_abs(vec![Value::Float(-3.14)]).unwrap();
        assert_eq!(result, Value::Float(3.14));
    }

    #[test]
    fn test_abs_wrong_type() {
        let result = builtin_abs(vec![Value::String("42".to_string())]);
        assert!(result.is_err());
    }

    #[test]
    fn test_abs_wrong_count() {
        let result = builtin_abs(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_ceil_integer() {
        let result = builtin_ceil(vec![Value::Integer(42)]).unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_ceil_float_positive() {
        let result = builtin_ceil(vec![Value::Float(3.14)]).unwrap();
        assert_eq!(result, Value::Integer(4));
    }

    #[test]
    fn test_ceil_float_negative() {
        let result = builtin_ceil(vec![Value::Float(-3.14)]).unwrap();
        assert_eq!(result, Value::Integer(-3));
    }

    #[test]
    fn test_ceil_float_whole() {
        let result = builtin_ceil(vec![Value::Float(3.0)]).unwrap();
        assert_eq!(result, Value::Integer(3));
    }

    #[test]
    fn test_ceil_wrong_type() {
        let result = builtin_ceil(vec![Value::String("42".to_string())]);
        assert!(result.is_err());
    }

    #[test]
    fn test_floor_integer() {
        let result = builtin_floor(vec![Value::Integer(-42)]).unwrap();
        assert_eq!(result, Value::Integer(-42));
    }

    #[test]
    fn test_floor_float_positive() {
        let result = builtin_floor(vec![Value::Float(3.99)]).unwrap();
        assert_eq!(result, Value::Integer(3));
    }

    #[test]
    fn test_floor_float_negative() {
        let result = builtin_floor(vec![Value::Float(-3.14)]).unwrap();
        assert_eq!(result, Value::Integer(-4));
    }

    #[test]
    fn test_floor_wrong_type() {
        let result = builtin_floor(vec![Value::String("42".to_string())]);
        assert!(result.is_err());
    }

    #[test]
    fn test_round_integer() {
        let result = builtin_round(vec![Value::Integer(42)]).unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_round_float_down() {
        let result = builtin_round(vec![Value::Float(3.4)]).unwrap();
        assert_eq!(result, Value::Integer(3));
    }

    #[test]
    fn test_round_float_up() {
        let result = builtin_round(vec![Value::Float(3.5)]).unwrap();
        assert_eq!(result, Value::Integer(4));
    }

    #[test]
    fn test_round_float_negative() {
        let result = builtin_round(vec![Value::Float(-3.5)]).unwrap();
        assert_eq!(result, Value::Integer(-4));
    }

    #[test]
    fn test_round_wrong_type() {
        let result = builtin_round(vec![Value::String("42".to_string())]);
        assert!(result.is_err());
    }
}
