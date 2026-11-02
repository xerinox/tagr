use std::{iter::Peekable, str::CharIndices};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParsingError {
    #[error("Expected closing parenthesis at span {span:?}, found {kind:?}")]
    UnmatchedParens { span: Span, kind: TokenKind },
    #[error("Expected closing parenthesis, found end of query")]
    MissingParens,
    #[error("Expected word at span {span:?}, found {kind:?}")]
    ExpectedWord { span: Span, kind: TokenKind },
    #[error("Expected word, found end of query")]
    MissingWord,
    #[error("Unexpected token at span: {span:?}: {kind:?}")]
    UnexpectedToken{span: Span, kind: TokenKind},
    #[error("Unexpected end of query")]
    UnexpectedEnd
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TokenKind {
    Word(String),
    And,
    Or,
    Minus,
    Not,
    LParen,
    RParen,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Expr {
    Word(String),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
    Not(Box<Self>),
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    #[must_use]
    pub const fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    /// # Errors
    pub fn parse_expr(&mut self) -> Result<Expr, ParsingError> {
        self.parse_or()
    }

    /// # Errors
    pub fn parse_or(&mut self) -> Result<Expr, ParsingError> {
        let mut left = self.parse_and()?;
        while let Some(Token {
            kind: TokenKind::Or,
            ..
        }) = self.peek()
        {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParsingError> {
        let mut left = self.parse_unary()?;
        while let Some(token) = self.peek() {
            if matches!(&token.kind, TokenKind::Or | TokenKind::RParen) {
                break;
            }
            if matches!(&token.kind, TokenKind::And) {
                self.advance();
            }
            let right = self.parse_unary()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParsingError> {
        if let Some(Token { kind, .. }) = self.peek() {
            if matches!(kind, TokenKind::Not | TokenKind::Minus) {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Not(Box::new(expr)))
            } else {
                self.parse_primary()
            }
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParsingError> {
        match self.peek() {
            Some(Token {
                kind: TokenKind::Word(_),
                ..
            }) => {
                let w = self.expect_word()?;
                Ok(Expr::Word(w))
            }
            Some(Token {
                kind: TokenKind::LParen,
                ..
            }) => {
                self.advance();
                let expr = self.parse_expr()?;
                match self.advance() {
                    Some(Token {
                        kind: TokenKind::RParen,
                        ..
                    }) => Ok(expr),
                    Some(tok) => Err(ParsingError::UnmatchedParens {
                        span: tok.span,
                        kind: tok.kind,
                    }),
                    None => Err(ParsingError::MissingParens),
                }
            }
            Some(tok) => {
                let err = ParsingError::UnexpectedToken { span: tok.span, kind: tok.kind.clone() };
                self.advance();
                Err(err)
            }
            None => Err(ParsingError::UnexpectedEnd),
        }
    }

    fn expect_word(&mut self) -> Result<String, ParsingError> {
        match self.advance() {
            Some(Token {
                kind: TokenKind::Word(w),
                ..
            }) => Ok(w),
            Some(tok) => Err(ParsingError::ExpectedWord { span: tok.span, kind: tok.kind }),
            None => Err(ParsingError::MissingWord),
        }
    }
}

struct Lexer<'a> {
    input: Peekable<CharIndices<'a>>,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.char_indices().peekable(),
        }
    }

    fn next_token(&mut self) -> Option<Token> {
        loop {
            match self.input.peek() {
                Some(&(i, c)) => match c {
                    ' ' | '\t' | '\n' | '\r' => {
                        self.input.next();
                    }
                    '(' => {
                        self.input.next();
                        return Some(Token {
                            kind: TokenKind::LParen,
                            span: Span {
                                start: i,
                                end: i + 1,
                            },
                        });
                    }
                    ')' => {
                        self.input.next();
                        return Some(Token {
                            kind: TokenKind::RParen,
                            span: Span {
                                start: i,
                                end: i + 1,
                            },
                        });
                    }
                    '-' => {
                        self.input.next();
                        return Some(Token {
                            kind: TokenKind::Minus,
                            span: Span {
                                start: i,
                                end: i + 1,
                            },
                        });
                    }
                    _ => {
                        let start = i;
                        let mut word = String::new();

                        while let Some((_, c)) = self.input.next_if(|&(_, ch)| {
                            !ch.is_whitespace() && ch != '(' && ch != ')' && ch != '-'
                        }) {
                            word.push(c);
                        }
                        let end = start + word.len();
                        let kind = match word.to_lowercase().as_str() {
                            "and" => TokenKind::And,
                            "or" => TokenKind::Or,
                            "not" => TokenKind::Not,
                            _ => TokenKind::Word(word),
                        };
                        return Some(Token {
                            kind,
                            span: Span { start, end },
                        });
                    }
                },
                None => return None,
            }
        }
    }
}

impl Iterator for Lexer<'_> {
    type Item = Token;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

impl Expr {
    #[must_use]
    pub fn compile_to_fts5(&self) -> String {
        match self {
            Self::Word(w) => w.strip_prefix('@').map_or_else(
                || format!("\"{w}*\""),
                |stripped| format!("meta:\"{stripped}\""),
            ),
            Self::And(l, r) => format!("({} AND {})", (l.compile_to_fts5()), (r.compile_to_fts5())),
            Self::Or(l, r) => format!("({} OR {})", l.compile_to_fts5(), r.compile_to_fts5()),
            Self::Not(e) => {
                let inner = e.compile_to_fts5();
                // Avoid double parentheses if the inner expression is already wrapped
                if inner.starts_with('(') && inner.ends_with(')') {
                    format!("NOT {inner}")
                } else {
                    format!("NOT ({inner})")
                }
            }
        }
    }
}

/// # Errors
pub fn parse(input: &str) -> Result<Expr, ParsingError> {
    let tokens = Lexer::new(input).collect();
    let mut parser = Parser::new(tokens);
    parser.parse_expr()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_parser_and_compiler() {
        let input = "docs and @category:work not (text or pdf)";
        let ast = parse(input).unwrap();
        let fts5 = &ast.compile_to_fts5();

        assert_eq!(
            fts5,
            "((\"docs*\" AND meta:\"category:work\") AND NOT (\"text*\" OR \"pdf*\"))"
        );
    }

    #[test]
    fn test_implicit_and_and_operators() {
        let input = "rust cli search @tag:code";
        let ast = parse(input).unwrap();
        let fts5 = &ast.compile_to_fts5();

        assert_eq!(
            fts5,
            "(((\"rust*\" AND \"cli*\") AND \"search*\") AND meta:\"tag:code\")"
        );
    }

    #[test]
    fn test_minus_as_not_operator() {
        let input = "(document -journal)";
        let ast = parse(input).unwrap();
        let fts5 = &ast.compile_to_fts5();

        assert_eq!(fts5, "(\"document*\" AND NOT \"journal*\")");
    }
}
