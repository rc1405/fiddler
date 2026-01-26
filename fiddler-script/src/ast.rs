//! Abstract Syntax Tree (AST) definitions for FiddlerScript.
//!
//! This module contains all the node types that make up the parsed representation
//! of a FiddlerScript program.

use crate::error::Position;

/// A complete FiddlerScript program.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// The statements that make up the program
    pub statements: Vec<Statement>,
}

impl Program {
    /// Create a new program with the given statements.
    pub fn new(statements: Vec<Statement>) -> Self {
        Self { statements }
    }
}

/// A statement in FiddlerScript.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// Variable declaration: `let x = expr;`
    Let {
        name: String,
        value: Expression,
        position: Position,
    },

    /// If statement: `if (condition) { ... } else { ... }`
    If {
        condition: Expression,
        then_block: Block,
        else_block: Option<ElseClause>,
        position: Position,
    },

    /// For loop: `for (init; condition; update) { ... }`
    For {
        init: Option<Box<Statement>>,
        condition: Option<Expression>,
        update: Option<Expression>,
        body: Block,
        position: Position,
    },

    /// Return statement: `return expr;`
    Return {
        value: Option<Expression>,
        position: Position,
    },

    /// Function definition: `fn name(params) { ... }`
    Function {
        name: String,
        params: Vec<String>,
        body: Block,
        position: Position,
    },

    /// Expression statement: `expr;`
    Expression {
        expression: Expression,
        position: Position,
    },

    /// Block statement: `{ ... }`
    Block(Block),
}

/// An else clause can be either a block or an else-if chain.
#[derive(Debug, Clone, PartialEq)]
pub enum ElseClause {
    /// else { ... }
    Block(Block),
    /// else if (...) { ... }
    ElseIf(Box<Statement>),
}

/// A block of statements.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The statements in the block
    pub statements: Vec<Statement>,
    /// Position of the opening brace
    pub position: Position,
}

impl Block {
    /// Create a new block with the given statements.
    pub fn new(statements: Vec<Statement>, position: Position) -> Self {
        Self {
            statements,
            position,
        }
    }
}

/// An expression in FiddlerScript.
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    /// Integer literal: `42`
    Integer { value: i64, position: Position },

    /// String literal: `"hello"`
    String { value: String, position: Position },

    /// Boolean literal: `true` or `false`
    Boolean { value: bool, position: Position },

    /// Variable reference: `x`
    Identifier { name: String, position: Position },

    /// Binary operation: `a + b`
    Binary {
        left: Box<Expression>,
        operator: BinaryOp,
        right: Box<Expression>,
        position: Position,
    },

    /// Unary operation: `-x` or `!x`
    Unary {
        operator: UnaryOp,
        operand: Box<Expression>,
        position: Position,
    },

    /// Assignment: `x = value`
    Assignment {
        name: String,
        value: Box<Expression>,
        position: Position,
    },

    /// Function call: `foo(args)`
    Call {
        function: String,
        arguments: Vec<Expression>,
        position: Position,
    },

    /// Grouped expression: `(expr)`
    Grouped {
        expression: Box<Expression>,
        position: Position,
    },
}

impl Expression {
    /// Get the position of this expression.
    pub fn position(&self) -> Position {
        match self {
            Expression::Integer { position, .. }
            | Expression::String { position, .. }
            | Expression::Boolean { position, .. }
            | Expression::Identifier { position, .. }
            | Expression::Binary { position, .. }
            | Expression::Unary { position, .. }
            | Expression::Assignment { position, .. }
            | Expression::Call { position, .. }
            | Expression::Grouped { position, .. } => *position,
        }
    }
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,

    // Comparison
    Equal,
    NotEqual,
    LessThan,
    LessEqual,
    GreaterThan,
    GreaterEqual,

    // Logical
    And,
    Or,
}

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Modulo => "%",
            BinaryOp::Equal => "==",
            BinaryOp::NotEqual => "!=",
            BinaryOp::LessThan => "<",
            BinaryOp::LessEqual => "<=",
            BinaryOp::GreaterThan => ">",
            BinaryOp::GreaterEqual => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
        };
        write!(f, "{}", s)
    }
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Logical negation: `!`
    Not,
    /// Arithmetic negation: `-`
    Negate,
}

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            UnaryOp::Not => "!",
            UnaryOp::Negate => "-",
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_creation() {
        let program = Program::new(vec![]);
        assert!(program.statements.is_empty());
    }

    #[test]
    fn test_binary_op_display() {
        assert_eq!(BinaryOp::Add.to_string(), "+");
        assert_eq!(BinaryOp::Equal.to_string(), "==");
        assert_eq!(BinaryOp::And.to_string(), "&&");
    }

    #[test]
    fn test_unary_op_display() {
        assert_eq!(UnaryOp::Not.to_string(), "!");
        assert_eq!(UnaryOp::Negate.to_string(), "-");
    }

    #[test]
    fn test_expression_position() {
        let pos = Position::new(1, 1, 0);
        let expr = Expression::Integer {
            value: 42,
            position: pos,
        };
        assert_eq!(expr.position(), pos);
    }
}
