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
        /// The variable name being declared
        name: String,
        /// The initial value expression
        value: Expression,
        /// Source position of the let keyword
        position: Position,
    },

    /// If statement: `if (condition) { ... } else { ... }`
    If {
        /// The condition expression to evaluate
        condition: Expression,
        /// The block to execute if condition is true
        then_block: Block,
        /// Optional else clause (block or else-if)
        else_block: Option<ElseClause>,
        /// Source position of the if keyword
        position: Position,
    },

    /// For loop: `for (init; condition; update) { ... }`
    For {
        /// Optional initialization statement
        init: Option<Box<Statement>>,
        /// Optional loop condition expression
        condition: Option<Expression>,
        /// Optional update expression
        update: Option<Expression>,
        /// The loop body
        body: Block,
        /// Source position of the for keyword
        position: Position,
    },

    /// Return statement: `return expr;`
    Return {
        /// Optional return value expression
        value: Option<Expression>,
        /// Source position of the return keyword
        position: Position,
    },

    /// Function definition: `fn name(params) { ... }`
    Function {
        /// The function name
        name: String,
        /// Parameter names
        params: Vec<String>,
        /// The function body
        body: Block,
        /// Source position of the fn keyword
        position: Position,
    },

    /// Expression statement: `expr;`
    Expression {
        /// The expression to evaluate
        expression: Expression,
        /// Source position of the expression start
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
    Integer {
        /// The integer value
        value: i64,
        /// Source position
        position: Position,
    },

    /// String literal: `"hello"`
    String {
        /// The string value
        value: String,
        /// Source position
        position: Position,
    },

    /// Boolean literal: `true` or `false`
    Boolean {
        /// The boolean value
        value: bool,
        /// Source position
        position: Position,
    },

    /// Variable reference: `x`
    Identifier {
        /// The variable name
        name: String,
        /// Source position
        position: Position,
    },

    /// Binary operation: `a + b`
    Binary {
        /// Left operand
        left: Box<Expression>,
        /// The binary operator
        operator: BinaryOp,
        /// Right operand
        right: Box<Expression>,
        /// Source position
        position: Position,
    },

    /// Unary operation: `-x` or `!x`
    Unary {
        /// The unary operator
        operator: UnaryOp,
        /// The operand expression
        operand: Box<Expression>,
        /// Source position
        position: Position,
    },

    /// Assignment: `x = value`
    Assignment {
        /// The variable name being assigned
        name: String,
        /// The value expression
        value: Box<Expression>,
        /// Source position
        position: Position,
    },

    /// Function call: `foo(args)`
    Call {
        /// The function name
        function: String,
        /// The argument expressions
        arguments: Vec<Expression>,
        /// Source position
        position: Position,
    },

    /// Grouped expression: `(expr)`
    Grouped {
        /// The inner expression
        expression: Box<Expression>,
        /// Source position
        position: Position,
    },

    /// Array literal: `[1, 2, 3]`
    ArrayLiteral {
        /// The array element expressions
        elements: Vec<Expression>,
        /// Source position
        position: Position,
    },

    /// Dictionary literal: `{"key": value, "another": 42}`
    DictionaryLiteral {
        /// Key-value pairs where keys are string expressions
        pairs: Vec<(Expression, Expression)>,
        /// Source position
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
            | Expression::Grouped { position, .. }
            | Expression::ArrayLiteral { position, .. }
            | Expression::DictionaryLiteral { position, .. } => *position,
        }
    }
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Addition: `+`
    Add,
    /// Subtraction: `-`
    Subtract,
    /// Multiplication: `*`
    Multiply,
    /// Division: `/`
    Divide,
    /// Modulo: `%`
    Modulo,
    /// Equality: `==`
    Equal,
    /// Inequality: `!=`
    NotEqual,
    /// Less than: `<`
    LessThan,
    /// Less than or equal: `<=`
    LessEqual,
    /// Greater than: `>`
    GreaterThan,
    /// Greater than or equal: `>=`
    GreaterEqual,
    /// Logical and: `&&`
    And,
    /// Logical or: `||`
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
