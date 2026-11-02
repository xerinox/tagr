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
    pub fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_or()
    }

    pub fn parse_or(&mut self) -> Result<Expr, String> {
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

    fn parse_and(&mut self) -> Result<Expr, String> {
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

    fn parse_unary(&mut self) -> Result<Expr, String> {
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

    fn parse_primary(&mut self) -> Result<Expr, String> {
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
                    Some(tok) => Err(format!(
                        "Expected closing parenthesis at span {:?}, found {:?}",
                        tok.span, tok.kind
                    )),
                    None => Err("Expected closing parenthesis, found end of query".into()),
                }
            }
            Some(tok) => {
                let msg = format!("Unexpected token at span {:?},: {:?}", tok.span, tok.kind);
                self.advance();
                Err(msg)
            }
            None => Err("Unexpected end of query".into()),
        }
    }

    fn expect_word(&mut self) -> Result<String, String> {
        match self.advance() {
            Some(Token {
                kind: TokenKind::Word(w),
                ..
            }) => Ok(w),
            Some(tok) => Err(format!(
                "Expected word at span {:?}, found {:?}",
                tok.span, tok.kind
            )),
            None => Err("Expected word, found end of query".into()),
        }
    }
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some(&(i, c)) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '(' => {
                chars.next();
                tokens.push(Token {
                    kind: TokenKind::LParen,
                    span: Span {
                        start: i,
                        end: i + 1,
                    },
                });
            }
            ')' => {
                chars.next();
                tokens.push(Token {
                    kind: TokenKind::RParen,
                    span: Span {
                        start: i,
                        end: i + 1,
                    },
                });
            }
            '-' => {
                chars.next();
                tokens.push(Token {
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

                while let Some((_, c)) = chars
                    .next_if(|&(_, ch)| !ch.is_whitespace() && ch != '(' && ch != ')' && ch != '-')
                {
                    word.push(c);
                }
                let end = start + word.len();
                let kind = match word.to_lowercase().as_str() {
                    "and" => TokenKind::And,
                    "or" => TokenKind::Or,
                    "not" => TokenKind::Not,
                    _ => TokenKind::Word(word),
                };
                tokens.push(Token {
                    kind,
                    span: Span { start, end },
                });
            }
        }
    }
    tokens
}

impl Expr {
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

pub fn parse(input: &str) -> Result<Expr, String> {
    let tokens = tokenize(input);
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
