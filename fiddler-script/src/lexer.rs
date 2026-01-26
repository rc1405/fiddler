//! Lexer (tokenizer) for FiddlerScript.
//!
//! The lexer transforms source code into a stream of tokens that can be
//! consumed by the parser.

use crate::error::{LexError, Position};

/// A token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The type and value of the token
    pub kind: TokenKind,
    /// Position in source code
    pub position: Position,
}

impl Token {
    /// Create a new token.
    pub fn new(kind: TokenKind, position: Position) -> Self {
        Self { kind, position }
    }
}

/// The different kinds of tokens in FiddlerScript.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    /// Integer literal
    Integer(i64),
    /// String literal
    String(String),

    // Identifiers and keywords
    /// Identifier (variable or function name)
    Identifier(String),
    /// `let` keyword
    Let,
    /// `if` keyword
    If,
    /// `else` keyword
    Else,
    /// `for` keyword
    For,
    /// `fn` keyword
    Fn,
    /// `return` keyword
    Return,
    /// `true` keyword
    True,
    /// `false` keyword
    False,

    // Operators
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `=`
    Assign,
    /// `==`
    Equal,
    /// `!=`
    NotEqual,
    /// `<`
    LessThan,
    /// `<=`
    LessEqual,
    /// `>`
    GreaterThan,
    /// `>=`
    GreaterEqual,
    /// `!`
    Bang,
    /// `&&`
    And,
    /// `||`
    Or,

    // Delimiters
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `[`
    LeftBracket,
    /// `]`
    RightBracket,
    /// `,`
    Comma,
    /// `;`
    Semicolon,
    /// `:`
    Colon,
    /// `.`
    Dot,

    // Special
    /// End of file
    Eof,
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Integer(n) => write!(f, "{}", n),
            TokenKind::String(s) => write!(f, "\"{}\"", s),
            TokenKind::Identifier(s) => write!(f, "{}", s),
            TokenKind::Let => write!(f, "let"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::For => write!(f, "for"),
            TokenKind::Fn => write!(f, "fn"),
            TokenKind::Return => write!(f, "return"),
            TokenKind::True => write!(f, "true"),
            TokenKind::False => write!(f, "false"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::Assign => write!(f, "="),
            TokenKind::Equal => write!(f, "=="),
            TokenKind::NotEqual => write!(f, "!="),
            TokenKind::LessThan => write!(f, "<"),
            TokenKind::LessEqual => write!(f, "<="),
            TokenKind::GreaterThan => write!(f, ">"),
            TokenKind::GreaterEqual => write!(f, ">="),
            TokenKind::Bang => write!(f, "!"),
            TokenKind::And => write!(f, "&&"),
            TokenKind::Or => write!(f, "||"),
            TokenKind::LeftParen => write!(f, "("),
            TokenKind::RightParen => write!(f, ")"),
            TokenKind::LeftBrace => write!(f, "{{"),
            TokenKind::RightBrace => write!(f, "}}"),
            TokenKind::LeftBracket => write!(f, "["),
            TokenKind::RightBracket => write!(f, "]"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Semicolon => write!(f, ";"),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Eof => write!(f, "EOF"),
        }
    }
}

/// The lexer that tokenizes FiddlerScript source code.
pub struct Lexer<'a> {
    /// Source code being tokenized
    source: &'a str,
    /// Characters iterator with indices
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    /// Current line number (1-indexed)
    line: usize,
    /// Current column number (1-indexed)
    column: usize,
    /// Current byte offset
    offset: usize,
    /// Whether we've reached EOF
    at_eof: bool,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given source code.
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.char_indices().peekable(),
            line: 1,
            column: 1,
            offset: 0,
            at_eof: false,
        }
    }

    /// Get the current position in the source.
    fn position(&self) -> Position {
        Position::new(self.line, self.column, self.offset)
    }

    /// Advance to the next character.
    fn advance(&mut self) -> Option<char> {
        if let Some((idx, ch)) = self.chars.next() {
            self.offset = idx + ch.len_utf8();
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            Some(ch)
        } else {
            None
        }
    }

    /// Peek at the next character without consuming it.
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, ch)| *ch)
    }

    /// Peek at the character after the current one without consuming.
    fn peek_next(&self) -> Option<char> {
        let mut iter = self.chars.clone();
        iter.next(); // Skip current
        iter.peek().map(|(_, ch)| *ch)
    }

    /// Skip whitespace and comments.
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace
            while let Some(ch) = self.peek() {
                if ch.is_whitespace() {
                    self.advance();
                } else {
                    break;
                }
            }

            // Check for comment: // until end of line
            if self.peek() == Some('/') && self.peek_next() == Some('/') {
                // Skip until end of line
                self.advance(); // consume first /
                self.advance(); // consume second /
                while let Some(ch) = self.peek() {
                    if ch == '\n' {
                        break;
                    }
                    self.advance();
                }
                continue; // Check for more whitespace/comments
            }

            break;
        }
    }

    /// Tokenize an identifier or keyword.
    fn scan_identifier(&mut self, first_char: char, start_pos: Position) -> Token {
        let start_offset = self.offset - first_char.len_utf8();

        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                self.advance();
            } else {
                break;
            }
        }

        let text = &self.source[start_offset..self.offset];
        let kind = match text {
            "let" => TokenKind::Let,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "for" => TokenKind::For,
            "fn" => TokenKind::Fn,
            "return" => TokenKind::Return,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Identifier(text.to_string()),
        };

        Token::new(kind, start_pos)
    }

    /// Tokenize a number literal.
    fn scan_number(&mut self, first_char: char, start_pos: Position) -> Result<Token, LexError> {
        let start_offset = self.offset - first_char.len_utf8();

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }

        let text = &self.source[start_offset..self.offset];
        match text.parse::<i64>() {
            Ok(value) => Ok(Token::new(TokenKind::Integer(value), start_pos)),
            Err(_) => Err(LexError::InvalidNumber(start_pos)),
        }
    }

    /// Tokenize a string literal.
    fn scan_string(&mut self, start_pos: Position) -> Result<Token, LexError> {
        let mut value = String::new();

        loop {
            match self.advance() {
                Some('"') => break,
                Some('\\') => {
                    // Handle escape sequences
                    match self.advance() {
                        Some('n') => value.push('\n'),
                        Some('t') => value.push('\t'),
                        Some('r') => value.push('\r'),
                        Some('\\') => value.push('\\'),
                        Some('"') => value.push('"'),
                        Some(ch) => return Err(LexError::InvalidEscape(ch, self.position())),
                        None => return Err(LexError::UnterminatedString(start_pos)),
                    }
                }
                Some(ch) => value.push(ch),
                None => return Err(LexError::UnterminatedString(start_pos)),
            }
        }

        Ok(Token::new(TokenKind::String(value), start_pos))
    }

    /// Get the next token from the source.
    pub fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace_and_comments();

        let pos = self.position();

        let Some(ch) = self.advance() else {
            self.at_eof = true;
            return Ok(Token::new(TokenKind::Eof, pos));
        };

        let kind = match ch {
            // Single-character tokens
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            '[' => TokenKind::LeftBracket,
            ']' => TokenKind::RightBracket,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            ':' => TokenKind::Colon,
            '.' => TokenKind::Dot,

            // Potentially two-character tokens
            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Equal
                } else {
                    TokenKind::Assign
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::NotEqual
                } else {
                    TokenKind::Bang
                }
            }
            '<' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::LessEqual
                } else {
                    TokenKind::LessThan
                }
            }
            '>' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::GreaterThan
                }
            }
            '&' => {
                if self.peek() == Some('&') {
                    self.advance();
                    TokenKind::And
                } else {
                    return Err(LexError::UnexpectedCharacter(ch, pos));
                }
            }
            '|' => {
                if self.peek() == Some('|') {
                    self.advance();
                    TokenKind::Or
                } else {
                    return Err(LexError::UnexpectedCharacter(ch, pos));
                }
            }

            // String literal
            '"' => return self.scan_string(pos),

            // Number literal
            ch if ch.is_ascii_digit() => return self.scan_number(ch, pos),

            // Identifier or keyword
            ch if ch.is_alphabetic() || ch == '_' => {
                return Ok(self.scan_identifier(ch, pos));
            }

            // Unknown character
            _ => return Err(LexError::UnexpectedCharacter(ch, pos)),
        };

        Ok(Token::new(kind, pos))
    }

    /// Tokenize the entire source and return all tokens.
    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_source() {
        let mut lexer = Lexer::new("");
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, TokenKind::Eof);
    }

    #[test]
    fn test_single_tokens() {
        let mut lexer = Lexer::new("+ - * / % ( ) { } , ;");
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Plus));
        assert!(matches!(tokens[1].kind, TokenKind::Minus));
        assert!(matches!(tokens[2].kind, TokenKind::Star));
        assert!(matches!(tokens[3].kind, TokenKind::Slash));
        assert!(matches!(tokens[4].kind, TokenKind::Percent));
        assert!(matches!(tokens[5].kind, TokenKind::LeftParen));
        assert!(matches!(tokens[6].kind, TokenKind::RightParen));
        assert!(matches!(tokens[7].kind, TokenKind::LeftBrace));
        assert!(matches!(tokens[8].kind, TokenKind::RightBrace));
        assert!(matches!(tokens[9].kind, TokenKind::Comma));
        assert!(matches!(tokens[10].kind, TokenKind::Semicolon));
    }

    #[test]
    fn test_comparison_operators() {
        let mut lexer = Lexer::new("= == != < <= > >=");
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Assign));
        assert!(matches!(tokens[1].kind, TokenKind::Equal));
        assert!(matches!(tokens[2].kind, TokenKind::NotEqual));
        assert!(matches!(tokens[3].kind, TokenKind::LessThan));
        assert!(matches!(tokens[4].kind, TokenKind::LessEqual));
        assert!(matches!(tokens[5].kind, TokenKind::GreaterThan));
        assert!(matches!(tokens[6].kind, TokenKind::GreaterEqual));
    }

    #[test]
    fn test_logical_operators() {
        let mut lexer = Lexer::new("! && ||");
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Bang));
        assert!(matches!(tokens[1].kind, TokenKind::And));
        assert!(matches!(tokens[2].kind, TokenKind::Or));
    }

    #[test]
    fn test_keywords() {
        let mut lexer = Lexer::new("let if else for fn return true false");
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Let));
        assert!(matches!(tokens[1].kind, TokenKind::If));
        assert!(matches!(tokens[2].kind, TokenKind::Else));
        assert!(matches!(tokens[3].kind, TokenKind::For));
        assert!(matches!(tokens[4].kind, TokenKind::Fn));
        assert!(matches!(tokens[5].kind, TokenKind::Return));
        assert!(matches!(tokens[6].kind, TokenKind::True));
        assert!(matches!(tokens[7].kind, TokenKind::False));
    }

    #[test]
    fn test_identifier() {
        let mut lexer = Lexer::new("foo bar_123 _test");
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::Identifier(s) if s == "foo"));
        assert!(matches!(&tokens[1].kind, TokenKind::Identifier(s) if s == "bar_123"));
        assert!(matches!(&tokens[2].kind, TokenKind::Identifier(s) if s == "_test"));
    }

    #[test]
    fn test_integer() {
        let mut lexer = Lexer::new("42 0 12345");
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Integer(42)));
        assert!(matches!(tokens[1].kind, TokenKind::Integer(0)));
        assert!(matches!(tokens[2].kind, TokenKind::Integer(12345)));
    }

    #[test]
    fn test_string() {
        let mut lexer = Lexer::new(r#""hello" "world" "with\nescapes""#);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::String(s) if s == "hello"));
        assert!(matches!(&tokens[1].kind, TokenKind::String(s) if s == "world"));
        assert!(matches!(&tokens[2].kind, TokenKind::String(s) if s == "with\nescapes"));
    }

    #[test]
    fn test_comments() {
        let mut lexer = Lexer::new("let x = 10; // this is a comment\nlet y = 20;");
        let tokens = lexer.tokenize().unwrap();
        // Should have: let, x, =, 10, ;, let, y, =, 20, ;, EOF
        assert_eq!(tokens.len(), 11);
        assert!(matches!(tokens[0].kind, TokenKind::Let));
        assert!(matches!(tokens[5].kind, TokenKind::Let));
    }

    #[test]
    fn test_position_tracking() {
        let mut lexer = Lexer::new("let x\ny");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].position.line, 1);
        assert_eq!(tokens[0].position.column, 1);
        assert_eq!(tokens[1].position.line, 1);
        assert_eq!(tokens[1].position.column, 5);
        assert_eq!(tokens[2].position.line, 2);
        assert_eq!(tokens[2].position.column, 1);
    }

    #[test]
    fn test_unterminated_string() {
        let mut lexer = Lexer::new(r#""hello"#);
        let result = lexer.next_token();
        assert!(matches!(result, Err(LexError::UnterminatedString(_))));
    }

    #[test]
    fn test_unexpected_character() {
        let mut lexer = Lexer::new("@");
        let result = lexer.next_token();
        assert!(matches!(result, Err(LexError::UnexpectedCharacter('@', _))));
    }

    #[test]
    fn test_dot_token() {
        let mut lexer = Lexer::new(".");
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Dot));
    }

    #[test]
    fn test_method_call_tokens() {
        let mut lexer = Lexer::new("foo.bar()");
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::Identifier(s) if s == "foo"));
        assert!(matches!(tokens[1].kind, TokenKind::Dot));
        assert!(matches!(&tokens[2].kind, TokenKind::Identifier(s) if s == "bar"));
        assert!(matches!(tokens[3].kind, TokenKind::LeftParen));
        assert!(matches!(tokens[4].kind, TokenKind::RightParen));
    }
}
