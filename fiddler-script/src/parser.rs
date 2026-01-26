//! Parser for FiddlerScript.
//!
//! This module implements a recursive descent parser with Pratt parsing
//! for expression precedence handling.
//!
//! The parser transforms a token stream into an Abstract Syntax Tree (AST).

use crate::ast::{BinaryOp, Block, ElseClause, Expression, Program, Statement, UnaryOp};
use crate::error::ParseError;
use crate::lexer::{Token, TokenKind};

/// The parser for FiddlerScript.
pub struct Parser {
    /// The tokens to parse
    tokens: Vec<Token>,
    /// Current position in the token stream
    current: usize,
}

impl Parser {
    /// Create a new parser for the given tokens.
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    /// Parse a complete program.
    pub fn parse(&mut self) -> Result<Program, ParseError> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        Ok(Program::new(statements))
    }

    // === Helper methods ===

    /// Check if we've reached the end of the token stream.
    fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::Eof
    }

    /// Get the current token without consuming it.
    fn peek(&self) -> &Token {
        self.tokens.get(self.current).unwrap_or_else(|| {
            self.tokens
                .last()
                .expect("Token stream should have at least EOF")
        })
    }

    /// Get the previous token.
    fn previous(&self) -> &Token {
        &self.tokens[self.current.saturating_sub(1)]
    }

    /// Advance to the next token and return the current one.
    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    /// Check if the current token matches the given kind.
    fn check(&self, kind: &TokenKind) -> bool {
        if self.is_at_end() {
            return false;
        }
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind)
    }

    /// Consume the current token if it matches the given kind.
    fn match_token(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Expect the current token to match the given kind, or return an error.
    fn expect(&mut self, kind: &TokenKind, expected: &str) -> Result<&Token, ParseError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(ParseError::UnexpectedToken(
                self.peek().kind.to_string(),
                self.peek().position,
                expected.to_string(),
            ))
        }
    }

    // === Statement parsing ===

    /// Parse a statement.
    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match &self.peek().kind {
            TokenKind::Let => self.parse_let_statement(),
            TokenKind::If => self.parse_if_statement(),
            TokenKind::For => self.parse_for_statement(),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::Fn => self.parse_function_definition(),
            TokenKind::LeftBrace => {
                let block = self.parse_block()?;
                Ok(Statement::Block(block))
            }
            _ => self.parse_expression_statement(),
        }
    }

    /// Parse a let statement: `let x = expr;`
    fn parse_let_statement(&mut self) -> Result<Statement, ParseError> {
        let position = self.peek().position;
        self.advance(); // consume 'let'

        let name = match &self.peek().kind {
            TokenKind::Identifier(name) => name.clone(),
            _ => return Err(ParseError::ExpectedIdentifier(self.peek().position)),
        };
        self.advance();

        self.expect(&TokenKind::Assign, "=")?;
        let value = self.parse_expression()?;
        self.expect(&TokenKind::Semicolon, ";")?;

        Ok(Statement::Let {
            name,
            value,
            position,
        })
    }

    /// Parse an if statement: `if (condition) { ... } else { ... }`
    fn parse_if_statement(&mut self) -> Result<Statement, ParseError> {
        let position = self.peek().position;
        self.advance(); // consume 'if'

        self.expect(&TokenKind::LeftParen, "(")?;
        let condition = self.parse_expression()?;
        self.expect(&TokenKind::RightParen, ")")?;

        let then_block = self.parse_block()?;

        let else_block = if self.match_token(&TokenKind::Else) {
            if self.check(&TokenKind::If) {
                // else if
                Some(ElseClause::ElseIf(Box::new(self.parse_if_statement()?)))
            } else {
                // else block
                Some(ElseClause::Block(self.parse_block()?))
            }
        } else {
            None
        };

        Ok(Statement::If {
            condition,
            then_block,
            else_block,
            position,
        })
    }

    /// Parse a for statement: `for (init; condition; update) { ... }`
    fn parse_for_statement(&mut self) -> Result<Statement, ParseError> {
        let position = self.peek().position;
        self.advance(); // consume 'for'

        self.expect(&TokenKind::LeftParen, "(")?;

        // Init clause
        let init = if self.match_token(&TokenKind::Semicolon) {
            None
        } else if self.check(&TokenKind::Let) {
            Some(Box::new(self.parse_let_statement()?))
        } else {
            let expr = self.parse_expression()?;
            let pos = expr.position();
            self.expect(&TokenKind::Semicolon, ";")?;
            Some(Box::new(Statement::Expression {
                expression: expr,
                position: pos,
            }))
        };

        // Condition clause
        let condition = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect(&TokenKind::Semicolon, ";")?;

        // Update clause
        let update = if self.check(&TokenKind::RightParen) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect(&TokenKind::RightParen, ")")?;

        let body = self.parse_block()?;

        Ok(Statement::For {
            init,
            condition,
            update,
            body,
            position,
        })
    }

    /// Parse a return statement: `return expr;`
    fn parse_return_statement(&mut self) -> Result<Statement, ParseError> {
        let position = self.peek().position;
        self.advance(); // consume 'return'

        let value = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expression()?)
        };

        self.expect(&TokenKind::Semicolon, ";")?;

        Ok(Statement::Return { value, position })
    }

    /// Parse a function definition: `fn name(params) { ... }`
    fn parse_function_definition(&mut self) -> Result<Statement, ParseError> {
        let position = self.peek().position;
        self.advance(); // consume 'fn'

        let name = match &self.peek().kind {
            TokenKind::Identifier(name) => name.clone(),
            _ => return Err(ParseError::ExpectedIdentifier(self.peek().position)),
        };
        self.advance();

        self.expect(&TokenKind::LeftParen, "(")?;
        let params = self.parse_parameters()?;
        self.expect(&TokenKind::RightParen, ")")?;

        let body = self.parse_block()?;

        Ok(Statement::Function {
            name,
            params,
            body,
            position,
        })
    }

    /// Parse function parameters.
    fn parse_parameters(&mut self) -> Result<Vec<String>, ParseError> {
        let mut params = Vec::new();

        if !self.check(&TokenKind::RightParen) {
            loop {
                let name = match &self.peek().kind {
                    TokenKind::Identifier(name) => name.clone(),
                    _ => return Err(ParseError::ExpectedIdentifier(self.peek().position)),
                };
                self.advance();
                params.push(name);

                if !self.match_token(&TokenKind::Comma) {
                    break;
                }
            }
        }

        Ok(params)
    }

    /// Parse an expression statement: `expr;`
    fn parse_expression_statement(&mut self) -> Result<Statement, ParseError> {
        let expression = self.parse_expression()?;
        let position = expression.position();
        self.expect(&TokenKind::Semicolon, ";")?;
        Ok(Statement::Expression {
            expression,
            position,
        })
    }

    /// Parse a block: `{ statements }`
    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let position = self.peek().position;
        self.expect(&TokenKind::LeftBrace, "{")?;

        let mut statements = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        self.expect(&TokenKind::RightBrace, "}")?;
        Ok(Block::new(statements, position))
    }

    // === Expression parsing (Pratt parsing) ===

    /// Parse an expression.
    pub fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_assignment()
    }

    /// Parse assignment expression.
    fn parse_assignment(&mut self) -> Result<Expression, ParseError> {
        let expr = self.parse_or()?;

        if self.match_token(&TokenKind::Assign) {
            let position = self.previous().position;
            let value = self.parse_assignment()?;

            match expr {
                Expression::Identifier { name, .. } => {
                    return Ok(Expression::Assignment {
                        name,
                        value: Box::new(value),
                        position,
                    });
                }
                _ => return Err(ParseError::InvalidAssignmentTarget(position)),
            }
        }

        Ok(expr)
    }

    /// Parse logical OR expression.
    fn parse_or(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_and()?;

        while self.match_token(&TokenKind::Or) {
            let position = self.previous().position;
            let right = self.parse_and()?;
            left = Expression::Binary {
                left: Box::new(left),
                operator: BinaryOp::Or,
                right: Box::new(right),
                position,
            };
        }

        Ok(left)
    }

    /// Parse logical AND expression.
    fn parse_and(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_equality()?;

        while self.match_token(&TokenKind::And) {
            let position = self.previous().position;
            let right = self.parse_equality()?;
            left = Expression::Binary {
                left: Box::new(left),
                operator: BinaryOp::And,
                right: Box::new(right),
                position,
            };
        }

        Ok(left)
    }

    /// Parse equality expression.
    fn parse_equality(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_comparison()?;

        loop {
            let op = if self.match_token(&TokenKind::Equal) {
                BinaryOp::Equal
            } else if self.match_token(&TokenKind::NotEqual) {
                BinaryOp::NotEqual
            } else {
                break;
            };

            let position = self.previous().position;
            let right = self.parse_comparison()?;
            left = Expression::Binary {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
                position,
            };
        }

        Ok(left)
    }

    /// Parse comparison expression.
    fn parse_comparison(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_addition()?;

        loop {
            let op = if self.match_token(&TokenKind::LessThan) {
                BinaryOp::LessThan
            } else if self.match_token(&TokenKind::LessEqual) {
                BinaryOp::LessEqual
            } else if self.match_token(&TokenKind::GreaterThan) {
                BinaryOp::GreaterThan
            } else if self.match_token(&TokenKind::GreaterEqual) {
                BinaryOp::GreaterEqual
            } else {
                break;
            };

            let position = self.previous().position;
            let right = self.parse_addition()?;
            left = Expression::Binary {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
                position,
            };
        }

        Ok(left)
    }

    /// Parse addition/subtraction expression.
    fn parse_addition(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_multiplication()?;

        loop {
            let op = if self.match_token(&TokenKind::Plus) {
                BinaryOp::Add
            } else if self.match_token(&TokenKind::Minus) {
                BinaryOp::Subtract
            } else {
                break;
            };

            let position = self.previous().position;
            let right = self.parse_multiplication()?;
            left = Expression::Binary {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
                position,
            };
        }

        Ok(left)
    }

    /// Parse multiplication/division/modulo expression.
    fn parse_multiplication(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_unary()?;

        loop {
            let op = if self.match_token(&TokenKind::Star) {
                BinaryOp::Multiply
            } else if self.match_token(&TokenKind::Slash) {
                BinaryOp::Divide
            } else if self.match_token(&TokenKind::Percent) {
                BinaryOp::Modulo
            } else {
                break;
            };

            let position = self.previous().position;
            let right = self.parse_unary()?;
            left = Expression::Binary {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
                position,
            };
        }

        Ok(left)
    }

    /// Parse unary expression.
    fn parse_unary(&mut self) -> Result<Expression, ParseError> {
        if self.match_token(&TokenKind::Bang) {
            let position = self.previous().position;
            let operand = self.parse_unary()?;
            return Ok(Expression::Unary {
                operator: UnaryOp::Not,
                operand: Box::new(operand),
                position,
            });
        }

        if self.match_token(&TokenKind::Minus) {
            let position = self.previous().position;
            let operand = self.parse_unary()?;
            return Ok(Expression::Unary {
                operator: UnaryOp::Negate,
                operand: Box::new(operand),
                position,
            });
        }

        self.parse_call()
    }

    /// Parse function call.
    fn parse_call(&mut self) -> Result<Expression, ParseError> {
        let expr = self.parse_primary()?;

        // Check if this is a function call
        if let Expression::Identifier { name, position } = expr {
            if self.match_token(&TokenKind::LeftParen) {
                let arguments = self.parse_arguments()?;
                self.expect(&TokenKind::RightParen, ")")?;
                return Ok(Expression::Call {
                    function: name,
                    arguments,
                    position,
                });
            }
            return Ok(Expression::Identifier { name, position });
        }

        Ok(expr)
    }

    /// Parse function arguments.
    fn parse_arguments(&mut self) -> Result<Vec<Expression>, ParseError> {
        let mut args = Vec::new();

        if !self.check(&TokenKind::RightParen) {
            loop {
                args.push(self.parse_expression()?);
                if !self.match_token(&TokenKind::Comma) {
                    break;
                }
            }
        }

        Ok(args)
    }

    /// Parse primary expression.
    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        let token = self.peek().clone();

        match &token.kind {
            TokenKind::Integer(value) => {
                let value = *value;
                self.advance();
                Ok(Expression::Integer {
                    value,
                    position: token.position,
                })
            }
            TokenKind::String(value) => {
                let value = value.clone();
                self.advance();
                Ok(Expression::String {
                    value,
                    position: token.position,
                })
            }
            TokenKind::True => {
                self.advance();
                Ok(Expression::Boolean {
                    value: true,
                    position: token.position,
                })
            }
            TokenKind::False => {
                self.advance();
                Ok(Expression::Boolean {
                    value: false,
                    position: token.position,
                })
            }
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();
                Ok(Expression::Identifier {
                    name,
                    position: token.position,
                })
            }
            TokenKind::LeftParen => {
                let position = token.position;
                self.advance();
                let expression = self.parse_expression()?;
                self.expect(&TokenKind::RightParen, ")")?;
                Ok(Expression::Grouped {
                    expression: Box::new(expression),
                    position,
                })
            }
            TokenKind::Eof => Err(ParseError::UnexpectedEof(token.position)),
            _ => Err(ParseError::ExpectedExpression(token.position)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(source: &str) -> Result<Program, ParseError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer error");
        let mut parser = Parser::new(tokens);
        parser.parse()
    }

    #[test]
    fn test_let_statement() {
        let program = parse("let x = 10;").unwrap();
        assert_eq!(program.statements.len(), 1);
        assert!(matches!(
            &program.statements[0],
            Statement::Let { name, .. } if name == "x"
        ));
    }

    #[test]
    fn test_expression_statement() {
        let program = parse("42;").unwrap();
        assert_eq!(program.statements.len(), 1);
        assert!(matches!(
            &program.statements[0],
            Statement::Expression { .. }
        ));
    }

    #[test]
    fn test_binary_expression() {
        let program = parse("1 + 2 * 3;").unwrap();
        // Should parse as 1 + (2 * 3) due to precedence
        if let Statement::Expression { expression, .. } = &program.statements[0] {
            assert!(matches!(
                expression,
                Expression::Binary {
                    operator: BinaryOp::Add,
                    ..
                }
            ));
        } else {
            panic!("Expected expression statement");
        }
    }

    #[test]
    fn test_if_statement() {
        let program = parse("if (x > 0) { let y = 1; }").unwrap();
        assert!(matches!(&program.statements[0], Statement::If { .. }));
    }

    #[test]
    fn test_if_else_statement() {
        let program = parse("if (x > 0) { let y = 1; } else { let y = 2; }").unwrap();
        if let Statement::If { else_block, .. } = &program.statements[0] {
            assert!(else_block.is_some());
        } else {
            panic!("Expected if statement");
        }
    }

    #[test]
    fn test_for_statement() {
        let program = parse("for (let i = 0; i < 10; i = i + 1) { print(i); }").unwrap();
        assert!(matches!(&program.statements[0], Statement::For { .. }));
    }

    #[test]
    fn test_function_definition() {
        let program = parse("fn add(a, b) { return a + b; }").unwrap();
        if let Statement::Function { name, params, .. } = &program.statements[0] {
            assert_eq!(name, "add");
            assert_eq!(params, &["a", "b"]);
        } else {
            panic!("Expected function definition");
        }
    }

    #[test]
    fn test_function_call() {
        let program = parse("print(42);").unwrap();
        if let Statement::Expression { expression, .. } = &program.statements[0] {
            assert!(matches!(expression, Expression::Call { function, .. } if function == "print"));
        } else {
            panic!("Expected expression statement");
        }
    }

    #[test]
    fn test_unary_expression() {
        let program = parse("-42;").unwrap();
        if let Statement::Expression { expression, .. } = &program.statements[0] {
            assert!(matches!(
                expression,
                Expression::Unary {
                    operator: UnaryOp::Negate,
                    ..
                }
            ));
        } else {
            panic!("Expected expression statement");
        }
    }

    #[test]
    fn test_grouped_expression() {
        let program = parse("(1 + 2) * 3;").unwrap();
        if let Statement::Expression { expression, .. } = &program.statements[0] {
            if let Expression::Binary {
                left,
                operator: BinaryOp::Multiply,
                ..
            } = expression
            {
                assert!(matches!(left.as_ref(), Expression::Grouped { .. }));
            } else {
                panic!("Expected multiply expression");
            }
        } else {
            panic!("Expected expression statement");
        }
    }

    #[test]
    fn test_assignment_expression() {
        let program = parse("x = 10;").unwrap();
        if let Statement::Expression { expression, .. } = &program.statements[0] {
            assert!(matches!(expression, Expression::Assignment { name, .. } if name == "x"));
        } else {
            panic!("Expected expression statement");
        }
    }

    #[test]
    fn test_return_statement() {
        let program = parse("return 42;").unwrap();
        assert!(matches!(
            &program.statements[0],
            Statement::Return { value: Some(_), .. }
        ));
    }

    #[test]
    fn test_empty_return() {
        let program = parse("return;").unwrap();
        assert!(matches!(
            &program.statements[0],
            Statement::Return { value: None, .. }
        ));
    }
}
