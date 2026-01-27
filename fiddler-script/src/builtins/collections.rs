//! Collection built-in functions (arrays and dictionaries).

use indexmap::IndexMap;

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
        return Err(RuntimeError::wrong_argument_count(2, args.len()));
    }

    match &args[0] {
        Value::Array(arr) => {
            let mut new_arr = arr.clone();
            new_arr.push(args[1].clone());
            Ok(Value::Array(new_arr))
        }
        _ => Err(RuntimeError::invalid_argument(
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
///
/// # Errors
/// - Returns an error if the index is negative
pub fn builtin_get(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::wrong_argument_count(2, args.len()));
    }

    match (&args[0], &args[1]) {
        (Value::Array(arr), Value::Integer(idx)) => {
            if *idx < 0 {
                return Err(RuntimeError::invalid_argument(format!(
                    "Array index cannot be negative: {}",
                    idx
                )));
            }
            let idx = *idx as usize;
            Ok(arr.get(idx).cloned().unwrap_or(Value::Null))
        }
        (Value::String(s), Value::Integer(idx)) => {
            if *idx < 0 {
                return Err(RuntimeError::invalid_argument(format!(
                    "String index cannot be negative: {}",
                    idx
                )));
            }
            let idx = *idx as usize;
            Ok(s.chars()
                .nth(idx)
                .map(|c| Value::String(c.to_string()))
                .unwrap_or(Value::Null))
        }
        (Value::Dictionary(dict), Value::String(key)) => {
            Ok(dict.get(key).cloned().unwrap_or(Value::Null))
        }
        (Value::Array(_) | Value::String(_), _) => Err(RuntimeError::invalid_argument(
            "Array/string index must be an integer".to_string(),
        )),
        (Value::Dictionary(_), _) => Err(RuntimeError::invalid_argument(
            "Dictionary key must be a string".to_string(),
        )),
        _ => Err(RuntimeError::invalid_argument(
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
/// # Array Behavior
/// If the index is beyond the current array length, the array will be
/// automatically extended with `null` values to accommodate the new index.
///
/// # Examples
/// ```ignore
/// let arr = array(1, 2, 3);
/// let arr2 = set(arr, 5, 99);  // Results in [1, 2, 3, null, null, 99]
/// ```
///
/// # Returns
/// - A new array/dictionary with the value set
///
/// # Errors
/// - Returns an error if the array index is negative
pub fn builtin_set(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 3 {
        return Err(RuntimeError::wrong_argument_count(3, args.len()));
    }

    match (&args[0], &args[1]) {
        (Value::Array(arr), Value::Integer(idx)) => {
            if *idx < 0 {
                return Err(RuntimeError::invalid_argument(format!(
                    "Array index cannot be negative: {}",
                    idx
                )));
            }
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
        (Value::Array(_), _) => Err(RuntimeError::invalid_argument(
            "Array index must be an integer".to_string(),
        )),
        (Value::Dictionary(_), _) => Err(RuntimeError::invalid_argument(
            "Dictionary key must be a string".to_string(),
        )),
        _ => Err(RuntimeError::invalid_argument(
            "set() requires an array or dictionary as first argument".to_string(),
        )),
    }
}

/// Create an empty dictionary.
///
/// # Returns
/// - An empty dictionary (with insertion order preserved)
pub fn builtin_dict(_args: Vec<Value>) -> Result<Value, RuntimeError> {
    Ok(Value::Dictionary(IndexMap::new()))
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
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    match &args[0] {
        Value::Dictionary(dict) => {
            let keys: Vec<Value> = dict.keys().map(|k| Value::String(k.clone())).collect();
            Ok(Value::Array(keys))
        }
        _ => Err(RuntimeError::invalid_argument(
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
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
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
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
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
///
/// # Errors
/// - Returns an error if the array index is negative
pub fn builtin_delete(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::wrong_argument_count(2, args.len()));
    }

    match (&args[0], &args[1]) {
        (Value::Dictionary(dict), Value::String(key)) => {
            let mut new_dict = dict.clone();
            new_dict.shift_remove(key);
            Ok(Value::Dictionary(new_dict))
        }
        (Value::Array(arr), Value::Integer(idx)) => {
            if *idx < 0 {
                return Err(RuntimeError::invalid_argument(format!(
                    "Array index cannot be negative: {}",
                    idx
                )));
            }
            let idx = *idx as usize;
            if idx < arr.len() {
                let mut new_arr = arr.clone();
                new_arr.remove(idx);
                Ok(Value::Array(new_arr))
            } else {
                Ok(Value::Array(arr.clone()))
            }
        }
        _ => Err(RuntimeError::invalid_argument(
            "delete() requires a dictionary and string key, or array and integer index".to_string(),
        )),
    }
}

/// Check if a collection contains a value or key.
///
/// # Arguments
/// - For arrays: An array and a value to search for
/// - For dictionaries: A dictionary and a key to search for
///
/// # Returns
/// - `true` if the value/key exists, `false` otherwise
///
/// # Examples
/// ```ignore
/// let arr = [1, 2, 3];
/// contains(arr, 2);      // true
/// arr.contains(2);       // true (method syntax)
///
/// let dict = {"name": "Alice", "age": 30};
/// contains(dict, "name");  // true
/// dict.contains("name");   // true (method syntax)
/// ```
pub fn builtin_contains(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::wrong_argument_count(2, args.len()));
    }

    match (&args[0], &args[1]) {
        (Value::Array(arr), search_value) => {
            let found = arr.iter().any(|v| v == search_value);
            Ok(Value::Boolean(found))
        }
        (Value::Dictionary(dict), Value::String(key)) => Ok(Value::Boolean(dict.contains_key(key))),
        (Value::Dictionary(_), _) => Err(RuntimeError::invalid_argument(
            "Dictionary contains() requires a string key as second argument".to_string(),
        )),
        _ => Err(RuntimeError::invalid_argument(
            "contains() requires an array or dictionary as first argument".to_string(),
        )),
    }
}
