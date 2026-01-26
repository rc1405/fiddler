//! Collection built-in functions (arrays and dictionaries).

use crate::error::RuntimeError;
use crate::Value;

/// Create a new array from arguments.
///
/// # Arguments
/// - Any number of values to include in the array
///
/// # Returns
/// - An array containing all arguments
pub fn builtin_array(args: Vec<Value>) -> Result<Value, RuntimeError> {
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
pub fn builtin_push(args: Vec<Value>) -> Result<Value, RuntimeError> {
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
pub fn builtin_get(args: Vec<Value>) -> Result<Value, RuntimeError> {
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
pub fn builtin_set(args: Vec<Value>) -> Result<Value, RuntimeError> {
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
pub fn builtin_dict(_args: Vec<Value>) -> Result<Value, RuntimeError> {
    Ok(Value::Dictionary(std::collections::HashMap::new()))
}

/// Get all keys from a dictionary.
///
/// # Arguments
/// - A dictionary
///
/// # Returns
/// - An array of keys
pub fn builtin_keys(args: Vec<Value>) -> Result<Value, RuntimeError> {
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
pub fn builtin_is_array(args: Vec<Value>) -> Result<Value, RuntimeError> {
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
pub fn builtin_is_dict(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    Ok(Value::Boolean(args[0].is_dictionary()))
}

/// Delete a key from a dictionary or element from an array.
///
/// # Arguments
/// - A dictionary and key, or array and index
///
/// # Returns
/// - A new collection without the element
pub fn builtin_delete(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::WrongArgumentCount(2, args.len()));
    }

    match (&args[0], &args[1]) {
        (Value::Dictionary(dict), Value::String(key)) => {
            let mut new_dict = dict.clone();
            new_dict.remove(key);
            Ok(Value::Dictionary(new_dict))
        }
        (Value::Array(arr), Value::Integer(idx)) => {
            let idx = *idx as usize;
            if idx < arr.len() {
                let mut new_arr = arr.clone();
                new_arr.remove(idx);
                Ok(Value::Array(new_arr))
            } else {
                Ok(Value::Array(arr.clone()))
            }
        }
        _ => Err(RuntimeError::InvalidArgument(
            "delete() requires a dictionary and string key, or array and integer index".to_string(),
        )),
    }
}
