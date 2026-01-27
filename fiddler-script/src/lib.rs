//! # FiddlerScript
//!
//! A minimal C-style scripting language with a Rust-based interpreter.
//!
//! ## Features
//!
//! - Variables with `let` declarations (integers and strings)
//! - Control flow with `if-else` statements and `for` loops
//! - User-defined functions with `fn` syntax
//! - Built-in functions (starting with `print()`)
//! - Single-line comments with `//`
//!
//! ## Example
//!
//! ```
//! use fiddler_script::Interpreter;
//!
//! let source = r#"
//!     let x = 10;
//!     let y = 20;
//!     print(x + y);
//! "#;
//!
//! let mut interpreter = Interpreter::new();
//! interpreter.run(source).expect("Failed to run script");
//! ```
//!
//! ## Custom Built-in Functions
//!
//! You can extend the interpreter with custom built-in functions:
//!
//! ```
//! use fiddler_script::{Interpreter, Value, RuntimeError};
//! use std::collections::HashMap;
//!
//! let mut builtins: HashMap<String, fn(Vec<Value>) -> Result<Value, RuntimeError>> = HashMap::new();
//! builtins.insert("double".to_string(), |args| {
//!     if let Some(Value::Integer(n)) = args.first() {
//!         Ok(Value::Integer(n * 2))
//!     } else {
//!         Err(RuntimeError::invalid_argument("Expected integer"))
//!     }
//! });
//!
//! let mut interpreter = Interpreter::with_builtins(builtins);
//! ```

pub mod ast;
pub mod builtins;
pub mod error;
pub mod interpreter;
pub mod lexer;
pub mod parser;

use indexmap::IndexMap;

// Re-export main types for convenience
pub use ast::{Expression, Program, Statement};
pub use builtins::BuiltinFn;
pub use error::{FiddlerError, LexError, ParseError, RuntimeError};
pub use interpreter::Interpreter;
pub use lexer::{Lexer, Token};
pub use parser::Parser;

/// A runtime value in FiddlerScript.
#[derive(Debug, Clone)]
pub enum Value {
    /// Integer value
    Integer(i64),
    /// Float value (64-bit floating point)
    Float(f64),
    /// String value
    String(String),
    /// Boolean value
    Boolean(bool),
    /// Bytes value (raw binary data)
    Bytes(Vec<u8>),
    /// Array value (list of values)
    Array(Vec<Value>),
    /// Dictionary value (key-value pairs with insertion order preserved)
    Dictionary(IndexMap<String, Value>),
    /// Represents no value (e.g., from a function with no return)
    Null,
}

// Custom PartialEq to handle cross-type numeric comparisons and NaN
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b, // NaN != NaN per IEEE 754
            (Value::Integer(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Integer(b)) => *a == (*b as f64),
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Dictionary(a), Value::Dictionary(b)) => a == b,
            (Value::Null, Value::Null) => true,
            _ => false,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Integer(n) => write!(f, "{}", n),
            Value::Float(fl) => {
                if fl.is_nan() {
                    write!(f, "NaN")
                } else if fl.is_infinite() {
                    if fl.is_sign_positive() {
                        write!(f, "Infinity")
                    } else {
                        write!(f, "-Infinity")
                    }
                } else if fl.fract() == 0.0 && fl.abs() < 1e15 {
                    // Integer-valued floats: show as 1.0, 2.0, etc.
                    write!(f, "{:.1}", fl)
                } else {
                    // General floats: format with precision, strip trailing zeros
                    let s = format!("{:.10}", fl);
                    let s = s.trim_end_matches('0').trim_end_matches('.');
                    write!(f, "{}", s)
                }
            }
            Value::String(s) => write!(f, "{}", s),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Bytes(bytes) => write!(f, "<bytes: {} bytes>", bytes.len()),
            Value::Array(arr) => {
                write!(f, "[")?;
                let mut first = true;
                for v in arr {
                    if !first {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                    first = false;
                }
                write!(f, "]")
            }
            Value::Dictionary(dict) => {
                write!(f, "{{")?;
                let mut first = true;
                for (k, v) in dict {
                    if !first {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                    first = false;
                }
                write!(f, "}}")
            }
            Value::Null => write!(f, "null"),
        }
    }
}

impl Value {
    /// Convert the value to bytes representation.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Value::Integer(n) => n.to_string().into_bytes(),
            Value::Float(f) => f.to_string().into_bytes(),
            Value::String(s) => s.as_bytes().to_vec(),
            Value::Boolean(b) => b.to_string().into_bytes(),
            Value::Bytes(bytes) => bytes.clone(),
            Value::Array(_) | Value::Dictionary(_) => self.to_string().into_bytes(),
            Value::Null => Vec::new(),
        }
    }

    /// Check if value is a number (integer or float).
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Integer(_) | Value::Float(_))
    }

    /// Try to get as f64, converting integers to floats.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Integer(n) => Some(*n as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Try to get as i64, truncating floats.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Integer(n) => Some(*n),
            Value::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    /// Try to create a Value from bytes, interpreting as UTF-8 string.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Value::Bytes(bytes)
    }

    /// Convert to string, handling both String and Bytes variants.
    ///
    /// Returns an error if the value is not a String or Bytes type.
    /// For Bytes, invalid UTF-8 sequences are replaced with the Unicode
    /// replacement character.
    pub fn as_string_lossy(&self) -> Result<String, RuntimeError> {
        match self {
            Value::String(s) => Ok(s.clone()),
            Value::Bytes(b) => Ok(String::from_utf8_lossy(b).into_owned()),
            _ => Err(RuntimeError::invalid_argument(
                "Expected string or bytes value",
            )),
        }
    }

    /// Check if the value is an array.
    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    /// Check if the value is a dictionary.
    pub fn is_dictionary(&self) -> bool {
        matches!(self, Value::Dictionary(_))
    }

    /// Get as array if this value is an array.
    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Get as dictionary if this value is a dictionary.
    pub fn as_dictionary(&self) -> Option<&IndexMap<String, Value>> {
        match self {
            Value::Dictionary(dict) => Some(dict),
            _ => None,
        }
    }
}
