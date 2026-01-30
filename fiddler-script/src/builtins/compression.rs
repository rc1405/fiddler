//! Compression built-in functions (gzip, zlib, deflate).

use std::io::Read;

use flate2::read::{
    DeflateDecoder, DeflateEncoder, GzDecoder, GzEncoder, ZlibDecoder, ZlibEncoder,
};
use flate2::Compression;

use crate::error::RuntimeError;
use crate::Value;

/// Get compression level from optional second argument.
/// Returns default compression if no level specified.
fn get_compression_level(args: &[Value]) -> Result<Compression, RuntimeError> {
    if args.len() > 1 {
        match &args[1] {
            Value::Integer(n) if *n >= 0 && *n <= 9 => Ok(Compression::new(*n as u32)),
            Value::Integer(n) => Err(RuntimeError::invalid_argument(format!(
                "Compression level must be between 0 and 9, got {}",
                n
            ))),
            _ => Err(RuntimeError::invalid_argument(
                "Compression level must be an integer".to_string(),
            )),
        }
    } else {
        Ok(Compression::default())
    }
}

/// Compress data using gzip.
///
/// # Arguments
/// - Data to compress (string or bytes)
/// - Optional compression level (0-9), defaults to 6 if not provided
pub fn builtin_gzip_compress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.is_empty() || args.len() > 2 {
        return Err(RuntimeError::invalid_argument(
            "gzip_compress() requires 1 or 2 arguments".to_string(),
        ));
    }

    let input = args[0].to_bytes();
    let level = get_compression_level(&args)?;
    let mut output = Vec::new();
    let mut encoder = GzEncoder::new(&input[..], level);
    encoder
        .read_to_end(&mut output)
        .map_err(|e| RuntimeError::invalid_argument(format!("gzip compression failed: {}", e)))?;

    Ok(Value::Bytes(output))
}

/// Decompress gzip data.
pub fn builtin_gzip_decompress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    let input = args[0].to_bytes();
    let mut output = Vec::new();
    let mut decoder = GzDecoder::new(&input[..]);
    decoder
        .read_to_end(&mut output)
        .map_err(|e| RuntimeError::invalid_argument(format!("gzip decompression failed: {}", e)))?;

    Ok(Value::Bytes(output))
}

/// Compress data using zlib.
///
/// # Arguments
/// - Data to compress (string or bytes)
/// - Optional compression level (0-9), defaults to 6 if not provided
pub fn builtin_zlib_compress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.is_empty() || args.len() > 2 {
        return Err(RuntimeError::invalid_argument(
            "zlib_compress() requires 1 or 2 arguments".to_string(),
        ));
    }

    let input = args[0].to_bytes();
    let level = get_compression_level(&args)?;
    let mut output = Vec::new();
    let mut encoder = ZlibEncoder::new(&input[..], level);
    encoder
        .read_to_end(&mut output)
        .map_err(|e| RuntimeError::invalid_argument(format!("zlib compression failed: {}", e)))?;

    Ok(Value::Bytes(output))
}

/// Decompress zlib data.
pub fn builtin_zlib_decompress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    let input = args[0].to_bytes();
    let mut output = Vec::new();
    let mut decoder = ZlibDecoder::new(&input[..]);
    decoder
        .read_to_end(&mut output)
        .map_err(|e| RuntimeError::invalid_argument(format!("zlib decompression failed: {}", e)))?;

    Ok(Value::Bytes(output))
}

/// Compress data using deflate.
///
/// # Arguments
/// - Data to compress (string or bytes)
/// - Optional compression level (0-9), defaults to 6 if not provided
pub fn builtin_deflate_compress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.is_empty() || args.len() > 2 {
        return Err(RuntimeError::invalid_argument(
            "deflate_compress() requires 1 or 2 arguments".to_string(),
        ));
    }

    let input = args[0].to_bytes();
    let level = get_compression_level(&args)?;
    let mut output = Vec::new();
    let mut encoder = DeflateEncoder::new(&input[..], level);
    encoder.read_to_end(&mut output).map_err(|e| {
        RuntimeError::invalid_argument(format!("deflate compression failed: {}", e))
    })?;

    Ok(Value::Bytes(output))
}

/// Decompress deflate data.
pub fn builtin_deflate_decompress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::wrong_argument_count(1, args.len()));
    }

    let input = args[0].to_bytes();
    let mut output = Vec::new();
    let mut decoder = DeflateDecoder::new(&input[..]);
    decoder.read_to_end(&mut output).map_err(|e| {
        RuntimeError::invalid_argument(format!("deflate decompression failed: {}", e))
    })?;

    Ok(Value::Bytes(output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gzip_roundtrip() {
        let input = Value::String("hello world".to_string());
        let compressed = builtin_gzip_compress(vec![input]).unwrap();
        let decompressed = builtin_gzip_decompress(vec![compressed]).unwrap();
        assert_eq!(decompressed, Value::Bytes(b"hello world".to_vec()));
    }

    #[test]
    fn test_zlib_roundtrip() {
        let input = Value::String("hello world".to_string());
        let compressed = builtin_zlib_compress(vec![input]).unwrap();
        let decompressed = builtin_zlib_decompress(vec![compressed]).unwrap();
        assert_eq!(decompressed, Value::Bytes(b"hello world".to_vec()));
    }

    #[test]
    fn test_deflate_roundtrip() {
        let input = Value::String("hello world".to_string());
        let compressed = builtin_deflate_compress(vec![input]).unwrap();
        let decompressed = builtin_deflate_decompress(vec![compressed]).unwrap();
        assert_eq!(decompressed, Value::Bytes(b"hello world".to_vec()));
    }
}
