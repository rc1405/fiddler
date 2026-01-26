//! Compression built-in functions (gzip, zlib, deflate).

use std::io::Read;

use flate2::read::{
    DeflateDecoder, DeflateEncoder, GzDecoder, GzEncoder, ZlibDecoder, ZlibEncoder,
};
use flate2::Compression;

use crate::error::RuntimeError;
use crate::Value;

/// Compress data using gzip.
pub fn builtin_gzip_compress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let input = args[0].to_bytes();
    let mut output = Vec::new();
    let mut encoder = GzEncoder::new(&input[..], Compression::default());
    encoder
        .read_to_end(&mut output)
        .map_err(|e| RuntimeError::InvalidArgument(format!("gzip compression failed: {}", e)))?;

    Ok(Value::Bytes(output))
}

/// Decompress gzip data.
pub fn builtin_gzip_decompress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let input = args[0].to_bytes();
    let mut output = Vec::new();
    let mut decoder = GzDecoder::new(&input[..]);
    decoder
        .read_to_end(&mut output)
        .map_err(|e| RuntimeError::InvalidArgument(format!("gzip decompression failed: {}", e)))?;

    Ok(Value::Bytes(output))
}

/// Compress data using zlib.
pub fn builtin_zlib_compress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let input = args[0].to_bytes();
    let mut output = Vec::new();
    let mut encoder = ZlibEncoder::new(&input[..], Compression::default());
    encoder
        .read_to_end(&mut output)
        .map_err(|e| RuntimeError::InvalidArgument(format!("zlib compression failed: {}", e)))?;

    Ok(Value::Bytes(output))
}

/// Decompress zlib data.
pub fn builtin_zlib_decompress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let input = args[0].to_bytes();
    let mut output = Vec::new();
    let mut decoder = ZlibDecoder::new(&input[..]);
    decoder
        .read_to_end(&mut output)
        .map_err(|e| RuntimeError::InvalidArgument(format!("zlib decompression failed: {}", e)))?;

    Ok(Value::Bytes(output))
}

/// Compress data using deflate.
pub fn builtin_deflate_compress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let input = args[0].to_bytes();
    let mut output = Vec::new();
    let mut encoder = DeflateEncoder::new(&input[..], Compression::default());
    encoder
        .read_to_end(&mut output)
        .map_err(|e| RuntimeError::InvalidArgument(format!("deflate compression failed: {}", e)))?;

    Ok(Value::Bytes(output))
}

/// Decompress deflate data.
pub fn builtin_deflate_decompress(args: Vec<Value>) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::WrongArgumentCount(1, args.len()));
    }

    let input = args[0].to_bytes();
    let mut output = Vec::new();
    let mut decoder = DeflateDecoder::new(&input[..]);
    decoder.read_to_end(&mut output).map_err(|e| {
        RuntimeError::InvalidArgument(format!("deflate decompression failed: {}", e))
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
