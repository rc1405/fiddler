//! Error types for FiddlerScript.
//!
//! This module defines all error types used throughout the interpreter:
//! - [`LexError`] - Errors during tokenization
//! - [`ParseError`] - Errors during parsing
//! - [`RuntimeError`] - Errors during execution
//! - [`FiddlerError`] - Top-level error that wraps all error types

use thiserror::Error;

/// Position in source code for error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
    /// Byte offset in source
    pub offset: usize,
}

impl Position {
    /// Create a new position.
    pub fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

/// Errors that occur during lexical analysis (tokenization).
#[derive(Debug, Error, Clone, PartialEq)]
pub enum LexError {
    /// Unexpected character encountered
    #[error("Unexpected character '{0}' at {1}")]
    UnexpectedCharacter(char, Position),

    /// Unterminated string literal
    #[error("Unterminated string starting at {0}")]
    UnterminatedString(Position),

    /// Invalid escape sequence in string
    #[error("Invalid escape sequence '\\{0}' at {1}")]
    InvalidEscape(char, Position),

    /// Invalid number literal
    #[error("Invalid number literal at {0}")]
    InvalidNumber(Position),
}

/// Errors that occur during parsing.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ParseError {
    /// Unexpected token encountered
    #[error("Unexpected token {0} at {1}, expected {2}")]
    UnexpectedToken(String, Position, String),

    /// Unexpected end of input
    #[error("Unexpected end of input at {0}")]
    UnexpectedEof(Position),

    /// Expected expression but found something else
    #[error("Expected expression at {0}")]
    ExpectedExpression(Position),

    /// Expected identifier
    #[error("Expected identifier at {0}")]
    ExpectedIdentifier(Position),

    /// Expected semicolon
    #[error("Expected ';' at {0}")]
    ExpectedSemicolon(Position),

    /// Invalid assignment target
    #[error("Invalid assignment target at {0}")]
    InvalidAssignmentTarget(Position),
}

/// Errors that occur during runtime execution.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum RuntimeError {
    /// Undefined variable
    #[error("Undefined variable '{0}'")]
    UndefinedVariable(String),

    /// Undefined function
    #[error("Undefined function '{0}'")]
    UndefinedFunction(String),

    /// Type mismatch in operation
    #[error("Type mismatch: {0}")]
    TypeMismatch(String),

    /// Division by zero
    #[error("Division by zero")]
    DivisionByZero,

    /// Wrong number of arguments
    #[error("Expected {0} arguments but got {1}")]
    WrongArgumentCount(usize, usize),

    /// Invalid argument to function
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Return from function (used internally for control flow)
    #[error("Return outside of function")]
    ReturnOutsideFunction,
}

/// Top-level error type that wraps all FiddlerScript errors.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum FiddlerError {
    /// Lexer error
    #[error("Lex error: {0}")]
    Lex(#[from] LexError),

    /// Parser error
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),

    /// Runtime error
    #[error("Runtime error: {0}")]
    Runtime(#[from] RuntimeError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_display() {
        let pos = Position::new(5, 10, 50);
        assert_eq!(pos.to_string(), "line 5, column 10");
    }

    #[test]
    fn test_lex_error_display() {
        let err = LexError::UnexpectedCharacter('@', Position::new(1, 5, 4));
        assert!(err.to_string().contains('@'));
        assert!(err.to_string().contains("line 1"));
    }

    #[test]
    fn test_parse_error_display() {
        let err = ParseError::UnexpectedEof(Position::new(10, 1, 100));
        assert!(err.to_string().contains("end of input"));
    }

    #[test]
    fn test_runtime_error_display() {
        let err = RuntimeError::UndefinedVariable("foo".to_string());
        assert!(err.to_string().contains("foo"));
    }

    #[test]
    fn test_fiddler_error_from_lex() {
        let lex_err = LexError::InvalidNumber(Position::default());
        let err: FiddlerError = lex_err.into();
        assert!(matches!(err, FiddlerError::Lex(_)));
    }
}
