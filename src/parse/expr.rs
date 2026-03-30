//! Expression parser — literals, binary ops, calls, match, and the core `Parser` struct.

use crate::ast::{Expr, BinOp, MatchArm, StringPart};
use crate::errors::ParseError;
use super::tokenizer::Token;

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
                    Ok(Expr::String(s))
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
                        let pattern = self.parse_expr()?;
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

/// Check if a string contains interpolation expressions like {name} or {obj.field}.
/// Empty braces {} are NOT interpolation.
/// Content with non-identifier characters (colons, commas, spaces) is NOT interpolation.
/// Only {identifier} and {obj.field} patterns count as interpolation.
fn has_interpolation(s: &str) -> bool {
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut content = String::new();
            let mut found_close = false;
            while let Some(&c) = chars.peek() {
                if c == '}' {
                    chars.next();
                    found_close = true;
                    break;
                }
                content.push(c);
                chars.next();
            }
            if !found_close { continue; }
            let trimmed = content.trim();
            if trimmed.is_empty() { continue; }
            // Must start with a letter or underscore (not a digit)
            let first = trimmed.chars().next().unwrap();
            if !first.is_alphabetic() && first != '_' { continue; }
            // Only valid interpolation if content is an identifier path or method call
            // Allows: {name}, {user.age}, {value.toString()}, {item.to_log()}
            if trimmed.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '(' || c == ')') {
                return true;
            }
        }
    }
    false
}

/// Parse "hello {name}, age {age}" into StringInterp parts
fn parse_string_interp(s: &str) -> Expr {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            if !current.is_empty() {
                parts.push(StringPart::Literal(current.clone()));
                current.clear();
            }
            let mut expr_str = String::new();
            while let Some(&c) = chars.peek() {
                if c == '}' {
                    chars.next();
                    break;
                }
                expr_str.push(c);
                chars.next();
            }
            // Parse the expression inside braces — for simple cases, just an identifier
            let trimmed = expr_str.trim();
            if trimmed.contains('.') {
                // Method call or field access: parse as tokens
                let tokens = super::tokenize(trimmed);
                let mut p = Parser::new(tokens);
                // String interp expressions are simple — unwrap is safe here
                parts.push(StringPart::Expr(p.parse_expr().unwrap()));
            } else {
                parts.push(StringPart::Expr(Expr::Ident(trimmed.to_string())));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        parts.push(StringPart::Literal(current));
    }

    Expr::StringInterp(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::tokenize;

    #[test]
    fn parse_simple_string() {
        let mut p = Parser::new(tokenize("\"hello\""));
        assert_eq!(p.parse_expr().unwrap(), Expr::String("hello".into()));
    }

    #[test]
    fn parse_binop() {
        let mut p = Parser::new(tokenize("1 + 2"));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::BinOp { op: BinOp::Add, .. }));
    }

    #[test]
    fn parse_field_access() {
        let mut p = Parser::new(tokenize("user.name"));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::FieldAccess { field, .. } if field == "name"));
    }

    #[test]
    fn parse_method_call() {
        let mut p = Parser::new(tokenize("name.trim()"));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::Call { .. }));
    }

    #[test]
    fn parse_err_as_ident() {
        // err.X is now parsed as field access on the err variable
        // ErrRef is only used via ReturnErr in statement parser
        let mut p = Parser::new(tokenize("err.timeout"));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::FieldAccess { field, .. } if field == "timeout"));
    }

    #[test]
    fn parse_struct_literal() {
        let mut p = Parser::new(tokenize("Email { value: \"test\" }"));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::StructLit { name, .. } if name == "Email"));
    }

    #[test]
    fn parse_function_call() {
        let mut p = Parser::new(tokenize("greet(\"cam\")"));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::Call { .. }));
    }

    #[test]
    fn parse_chained_method() {
        let mut p = Parser::new(tokenize("raw.trim().to_upper()"));
        let expr = p.parse_expr().unwrap();
        // Should be Call(FieldAccess(Call(FieldAccess(Ident(raw), trim)), to_upper))
        assert!(matches!(expr, Expr::Call { .. }));
    }

    #[test]
    fn parse_error_on_bad_token() {
        let mut p = Parser::new(tokenize("->"));
        let result = p.parse_expr();
        assert!(result.is_err());
    }

    // ─── String interpolation edge cases ─────

    #[test]
    fn json_string_not_interpolated() {
        // "{key: value}" should be a plain string, not interpolation
        let mut p = Parser::new(tokenize(r#""{key: value}""#));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(&expr, Expr::String(s) if s == "{key: value}"),
            "JSON-like string should not be interpolated, got: {:?}", expr);
    }

    #[test]
    fn json_object_string_not_interpolated() {
        let mut p = Parser::new(tokenize(r#""{"name":"cam"}""#));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::String(_)),
            "JSON object string should not be interpolated, got: {:?}", expr);
    }

    #[test]
    fn empty_braces_not_interpolated() {
        let mut p = Parser::new(tokenize(r#""{}""#));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(&expr, Expr::String(s) if s == "{}"),
            "empty braces should not be interpolated, got: {:?}", expr);
    }

    #[test]
    fn valid_interpolation_works() {
        let mut p = Parser::new(tokenize(r#""hello {name}""#));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::StringInterp(_)),
            "valid interpolation should work, got: {:?}", expr);
    }

    #[test]
    fn dotted_interpolation_works() {
        let mut p = Parser::new(tokenize(r#""age is {user.age}""#));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::StringInterp(_)),
            "dotted interpolation should work, got: {:?}", expr);
    }

    #[test]
    fn braces_with_spaces_not_interpolated() {
        let mut p = Parser::new(tokenize(r#""{ not valid }""#));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::String(_)),
            "braces with spaces around content should not interpolate, got: {:?}", expr);
    }

    #[test]
    fn numeric_braces_not_interpolated() {
        let mut p = Parser::new(tokenize(r#""{123}""#));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::String(_)),
            "numeric braces should not be interpolated, got: {:?}", expr);
    }

    #[test]
    fn css_string_not_interpolated() {
        let mut p = Parser::new(tokenize(r#"".class { color: red; }""#));
        let expr = p.parse_expr().unwrap();
        assert!(matches!(expr, Expr::String(_)),
            "CSS-like string should not be interpolated, got: {:?}", expr);
    }
}
