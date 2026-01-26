//! Built-in functions for FiddlerScript.
//!
//! This module provides the default built-in functions and a system for
//! registering custom built-in functions.

mod bytes;
mod collections;
mod compression;
mod core;
mod encoding;
mod json;
mod strings;

use std::collections::HashMap;

use crate::error::RuntimeError;
use crate::Value;

// Re-export json_to_value for use by other modules
pub use json::json_to_value;

/// Type alias for built-in function signatures.
pub type BuiltinFn = fn(Vec<Value>) -> Result<Value, RuntimeError>;

/// Get the default built-in functions.
pub fn get_default_builtins() -> HashMap<String, BuiltinFn> {
    let mut builtins: HashMap<String, BuiltinFn> = HashMap::new();

    // Core functions
    builtins.insert("print".to_string(), core::builtin_print);
    builtins.insert("len".to_string(), core::builtin_len);
    builtins.insert("str".to_string(), core::builtin_str);
    builtins.insert("int".to_string(), core::builtin_int);
    builtins.insert("getenv".to_string(), core::builtin_getenv);

    // JSON
    builtins.insert("parse_json".to_string(), json::builtin_parse_json);

    // Bytes conversion
    builtins.insert(
        "bytes_to_string".to_string(),
        bytes::builtin_bytes_to_string,
    );
    builtins.insert("bytes".to_string(), bytes::builtin_bytes);

    // Collections
    builtins.insert("array".to_string(), collections::builtin_array);
    builtins.insert("push".to_string(), collections::builtin_push);
    builtins.insert("get".to_string(), collections::builtin_get);
    builtins.insert("set".to_string(), collections::builtin_set);
    builtins.insert("dict".to_string(), collections::builtin_dict);
    builtins.insert("keys".to_string(), collections::builtin_keys);
    builtins.insert("is_array".to_string(), collections::builtin_is_array);
    builtins.insert("is_dict".to_string(), collections::builtin_is_dict);
    builtins.insert("delete".to_string(), collections::builtin_delete);

    // Compression
    builtins.insert(
        "gzip_compress".to_string(),
        compression::builtin_gzip_compress,
    );
    builtins.insert(
        "gzip_decompress".to_string(),
        compression::builtin_gzip_decompress,
    );
    builtins.insert(
        "zlib_compress".to_string(),
        compression::builtin_zlib_compress,
    );
    builtins.insert(
        "zlib_decompress".to_string(),
        compression::builtin_zlib_decompress,
    );
    builtins.insert(
        "deflate_compress".to_string(),
        compression::builtin_deflate_compress,
    );
    builtins.insert(
        "deflate_decompress".to_string(),
        compression::builtin_deflate_decompress,
    );

    // Encoding
    builtins.insert("base64_encode".to_string(), encoding::builtin_base64_encode);
    builtins.insert("base64_decode".to_string(), encoding::builtin_base64_decode);

    // String methods
    builtins.insert("lines".to_string(), strings::builtin_lines);
    builtins.insert("capitalize".to_string(), strings::builtin_capitalize);
    builtins.insert("lowercase".to_string(), strings::builtin_lowercase);
    builtins.insert("uppercase".to_string(), strings::builtin_uppercase);
    builtins.insert("trim".to_string(), strings::builtin_trim);
    builtins.insert("trim_prefix".to_string(), strings::builtin_trim_prefix);
    builtins.insert("trim_suffix".to_string(), strings::builtin_trim_suffix);
    builtins.insert("has_prefix".to_string(), strings::builtin_has_prefix);
    builtins.insert("has_suffix".to_string(), strings::builtin_has_suffix);
    builtins.insert("split".to_string(), strings::builtin_split);
    builtins.insert("reverse".to_string(), strings::builtin_reverse);

    builtins
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_builtins() {
        let builtins = get_default_builtins();

        // Core
        assert!(builtins.contains_key("print"));
        assert!(builtins.contains_key("len"));
        assert!(builtins.contains_key("str"));
        assert!(builtins.contains_key("int"));
        assert!(builtins.contains_key("getenv"));

        // JSON
        assert!(builtins.contains_key("parse_json"));

        // Bytes
        assert!(builtins.contains_key("bytes_to_string"));
        assert!(builtins.contains_key("bytes"));

        // Collections
        assert!(builtins.contains_key("array"));
        assert!(builtins.contains_key("push"));
        assert!(builtins.contains_key("get"));
        assert!(builtins.contains_key("set"));
        assert!(builtins.contains_key("dict"));
        assert!(builtins.contains_key("keys"));
        assert!(builtins.contains_key("is_array"));
        assert!(builtins.contains_key("is_dict"));
        assert!(builtins.contains_key("delete"));

        // Compression
        assert!(builtins.contains_key("gzip_compress"));
        assert!(builtins.contains_key("gzip_decompress"));
        assert!(builtins.contains_key("zlib_compress"));
        assert!(builtins.contains_key("zlib_decompress"));
        assert!(builtins.contains_key("deflate_compress"));
        assert!(builtins.contains_key("deflate_decompress"));

        // Encoding
        assert!(builtins.contains_key("base64_encode"));
        assert!(builtins.contains_key("base64_decode"));

        // String methods
        assert!(builtins.contains_key("lines"));
        assert!(builtins.contains_key("capitalize"));
        assert!(builtins.contains_key("lowercase"));
        assert!(builtins.contains_key("uppercase"));
        assert!(builtins.contains_key("trim"));
        assert!(builtins.contains_key("trim_prefix"));
        assert!(builtins.contains_key("trim_suffix"));
        assert!(builtins.contains_key("has_prefix"));
        assert!(builtins.contains_key("has_suffix"));
        assert!(builtins.contains_key("split"));
        assert!(builtins.contains_key("reverse"));
    }
}
