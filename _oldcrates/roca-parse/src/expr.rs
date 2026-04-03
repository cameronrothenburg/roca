//! Expression parser — literals, binary ops, calls, match, and the core `Parser` struct.

use roca_ast::{Expr, BinOp, MatchArm, MatchPattern};
use roca_errors::ParseError;
use super::tokenizer::Token;
use super::string_interp::{has_interpolation, strip_escapes, parse_string_interp};

pub type ParseResult<T> = Result<T, ParseError>;

/// Parser state — shared cursor over token stream
pub struct Parser {
    pub tokens: Vec<Token>,
    pub pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::EOF)
    }

    pub fn peek_ahead(&self, n: usize) -> &Token {
        self.tokens.get(self.pos + n).unwrap_or(&Token::EOF)
    }

    pub fn advance(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::EOF);
        self.pos += 1;
        tok
    }

    pub fn expect(&mut self, expected: &Token) -> ParseResult<()> {
        let tok = self.advance();
        if &tok != expected {
            return Err(ParseError::new(
                format!("expected {:?}, got {:?}", expected, tok),
                self.pos,
            ));
        }
        Ok(())
    }

    pub fn expect_ident(&mut self) -> ParseResult<String> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            other => Err(ParseError::new(
                format!("expected identifier, got {:?}", other),
                self.pos,
            )),
        }
    }

    pub fn at(&self, tok: &Token) -> bool {
        self.peek() == tok
    }

    pub fn eat(&mut self, tok: &Token) -> bool {
        if self.at(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub fn err(&self, msg: impl Into<String>) -> ParseError {
        ParseError::new(msg, self.pos)
    }

    /// Collect consecutive DocComment tokens into a single doc string.
    /// Returns None if no doc comments are present.
    pub fn collect_doc(&mut self) -> Option<String> {
        let mut lines: Vec<String> = Vec::new();
        while let Token::DocComment(text) = self.peek() {
            lines.push(text.clone());
            self.advance();
        }
        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    /// Parse an expression
    pub fn parse_expr(&mut self) -> ParseResult<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_and()?;
        while self.at(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_equality()?;
        while self.at(&Token::And) {
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek() {
                Token::Eq => BinOp::Eq,
                Token::Neq => BinOp::Neq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek() {
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::Lte => BinOp::Lte,
                Token::Gte => BinOp::Gte,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> ParseResult<Expr> {
        // Unary minus: -expr
        if self.at(&Token::Minus) {
            self.advance();
            let expr = self.parse_postfix()?;
            return Ok(Expr::BinOp {
                left: Box::new(Expr::Number(0.0)),
                op: BinOp::Sub,
                right: Box::new(expr),
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> ParseResult<Expr> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek() {
                // Field access: expr.field
                Token::Dot => {
                    self.advance();
                    let field = self.expect_ident()?;
                    // Check if it's a method call: expr.field(args)
                    if self.at(&Token::LParen) {
                        self.advance();
                        let args = self.parse_args()?;
                        self.expect(&Token::RParen)?;
                        expr = Expr::Call {
                            target: Box::new(Expr::FieldAccess {
                                target: Box::new(expr),
                                field,
                            }),
                            args,
                        };
                    } else {
                        expr = Expr::FieldAccess {
                            target: Box::new(expr),
                            field,
                        };
                    }
                }
                // Direct call: expr(args)
                Token::LParen => {
                    self.advance();
                    let args = self.parse_args()?;
                    self.expect(&Token::RParen)?;
                    expr = Expr::Call {
                        target: Box::new(expr),
                        args,
                    };
                }
                // Index access: expr[index]
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    expr = Expr::Index {
                        target: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> ParseResult<Expr> {
        match self.peek().clone() {
            Token::StringLit(s) => {
                self.advance();
                if has_interpolation(&s) {
                    Ok(parse_string_interp(&s))
                } else {
                    Ok(Expr::String(strip_escapes(&s)))
                }
            }
            Token::NumberLit(n) => {
                self.advance();
                Ok(Expr::Number(n))
            }
            Token::BoolLit(b) => {
                self.advance();
                Ok(Expr::Bool(b))
            }
            Token::Fn => {
                // Closure: fn(x, y) -> expr
                self.advance();
                self.expect(&Token::LParen)?;
                let mut params = Vec::new();
                if !self.at(&Token::RParen) {
                    params.push(self.expect_ident()?);
                    while self.eat(&Token::Comma) {
                        params.push(self.expect_ident()?);
                    }
                }
                self.expect(&Token::RParen)?;
                self.expect(&Token::Arrow)?;
                let body = self.parse_expr()?;
                Ok(Expr::Closure { params, body: Box::new(body) })
            }
            Token::Null => {
                self.advance();
                Ok(Expr::Null)
            }
            Token::SelfKw => {
                self.advance();
                Ok(Expr::SelfRef)
            }
            Token::Err => {
                // Always treat err as an identifier — field access handled by postfix
                // ErrRef is only used via ReturnErr in statement parser
                self.advance();
                Ok(Expr::Ident("err".to_string()))
            }
            Token::Ok => {
                self.advance();
                Ok(Expr::Ident("Ok".to_string()))
            }
            Token::Ident(name) => {
                self.advance();
                // Check for struct literal: Name { field: value, ... }
                if name.chars().next().map_or(false, |c| c.is_uppercase())
                    && self.at(&Token::LBrace)
                    // Peek to distinguish block vs struct lit
                    && matches!(self.peek_ahead(1), Token::Ident(_))
                    && matches!(self.peek_ahead(2), Token::Colon)
                {
                    self.advance(); // {
                    let mut fields = Vec::new();
                    while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                        let field_name = self.expect_ident()?;
                        self.expect(&Token::Colon)?;
                        let value = self.parse_expr()?;
                        fields.push((field_name, value));
                        self.eat(&Token::Comma);
                    }
                    self.expect(&Token::RBrace)?;
                    Ok(Expr::StructLit { name, fields })
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            Token::LBracket => {
                // Array literal: [1, 2, 3]
                self.advance();
                let mut elements = Vec::new();
                if !self.at(&Token::RBracket) {
                    elements.push(self.parse_expr()?);
                    while self.eat(&Token::Comma) {
                        if self.at(&Token::RBracket) { break; }
                        elements.push(self.parse_expr()?);
                    }
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::Array(elements))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::Wait => {
                self.advance();
                let expr = self.parse_expr()?;
                Ok(Expr::Await(Box::new(expr)))
            }
            Token::Not => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Not(Box::new(expr)))
            }
            Token::Match => {
                self.advance();
                let value = self.parse_expr()?;
                self.expect(&Token::LBrace)?;
                let mut arms = Vec::new();
                while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                    if self.peek() == &Token::Ident("_".to_string()) {
                        // Default arm: _ => expr
                        self.advance();
                        self.expect(&Token::FatArrow)?;
                        let result = self.parse_expr()?;
                        arms.push(MatchArm { pattern: None, value: result });
                    } else {
                        let pattern = self.parse_match_pattern()?;
                        self.expect(&Token::FatArrow)?;
                        let result = self.parse_expr()?;
                        arms.push(MatchArm { pattern: Some(pattern), value: result });
                    }
                }
                self.expect(&Token::RBrace)?;
                Ok(Expr::Match {
                    value: Box::new(value),
                    arms,
                })
            }
            other => Err(self.err(format!("unexpected token in expression: {:?}", other))),
        }
    }

    /// Parse a match pattern: value literal, or Enum.Variant(binding, ...)
    fn parse_match_pattern(&mut self) -> ParseResult<MatchPattern> {
        // Check for Enum.Variant(bindings) pattern
        if let Token::Ident(_) = self.peek() {
            let saved = self.pos;
            let name = self.expect_ident()?;

            if self.eat(&Token::Dot) {
                // Enum.Variant or Enum.Variant(bindings)
                let variant = self.expect_ident()?;
                let mut bindings = Vec::new();
                if self.eat(&Token::LParen) {
                    if !self.at(&Token::RParen) {
                        bindings.push(self.expect_ident()?);
                        while self.eat(&Token::Comma) {
                            bindings.push(self.expect_ident()?);
                        }
                    }
                    self.expect(&Token::RParen)?;
                }
                return Ok(MatchPattern::Variant { enum_name: name, variant, bindings });
            }
            // Not a variant pattern — backtrack and parse as value
            self.pos = saved;
        }
        let expr = self.parse_expr()?;
        Ok(MatchPattern::Value(expr))
    }

    pub fn parse_args(&mut self) -> ParseResult<Vec<Expr>> {
        let mut args = Vec::new();
        if !self.at(&Token::RParen) {
            args.push(self.parse_expr()?);
            while self.eat(&Token::Comma) {
                args.push(self.parse_expr()?);
            }
        }
        Ok(args)
    }
}

#[cfg(test)]
#[path = "expr_tests.rs"]
mod tests;
