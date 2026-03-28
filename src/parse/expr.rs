use crate::ast::{Expr, BinOp, MatchArm};
use super::tokenizer::Token;

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

    pub fn expect(&mut self, expected: &Token) {
        let tok = self.advance();
        if &tok != expected {
            panic!("expected {:?}, got {:?}", expected, tok);
        }
    }

    pub fn expect_ident(&mut self) -> String {
        match self.advance() {
            Token::Ident(s) => s,
            other => panic!("expected identifier, got {:?}", other),
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

    /// Parse an expression
    pub fn parse_expr(&mut self) -> Expr {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Expr {
        let mut left = self.parse_and();
        while self.at(&Token::Or) {
            self.advance();
            let right = self.parse_and();
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_and(&mut self) -> Expr {
        let mut left = self.parse_equality();
        while self.at(&Token::And) {
            self.advance();
            let right = self.parse_equality();
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_equality(&mut self) -> Expr {
        let mut left = self.parse_comparison();
        loop {
            let op = match self.peek() {
                Token::Eq => BinOp::Eq,
                Token::Neq => BinOp::Neq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison();
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut left = self.parse_additive();
        loop {
            let op = match self.peek() {
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::Lte => BinOp::Lte,
                Token::Gte => BinOp::Gte,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive();
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_additive(&mut self) -> Expr {
        let mut left = self.parse_multiplicative();
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative();
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_multiplicative(&mut self) -> Expr {
        let mut left = self.parse_unary();
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary();
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_unary(&mut self) -> Expr {
        // Unary minus: -expr
        if self.at(&Token::Minus) {
            self.advance();
            let expr = self.parse_postfix();
            return Expr::BinOp {
                left: Box::new(Expr::Number(0.0)),
                op: BinOp::Sub,
                right: Box::new(expr),
            };
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_primary();

        loop {
            match self.peek() {
                // Field access: expr.field
                Token::Dot => {
                    self.advance();
                    let field = self.expect_ident();
                    // Check if it's a method call: expr.field(args)
                    if self.at(&Token::LParen) {
                        self.advance();
                        let args = self.parse_args();
                        self.expect(&Token::RParen);
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
                    let args = self.parse_args();
                    self.expect(&Token::RParen);
                    expr = Expr::Call {
                        target: Box::new(expr),
                        args,
                    };
                }
                // Index access: expr[index]
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_expr();
                    self.expect(&Token::RBracket);
                    expr = Expr::Index {
                        target: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                _ => break,
            }
        }

        expr
    }

    fn parse_primary(&mut self) -> Expr {
        match self.peek().clone() {
            Token::StringLit(s) => {
                self.advance();
                Expr::String(s)
            }
            Token::NumberLit(n) => {
                self.advance();
                Expr::Number(n)
            }
            Token::BoolLit(b) => {
                self.advance();
                Expr::Bool(b)
            }
            Token::SelfKw => {
                self.advance();
                Expr::SelfRef
            }
            Token::Err => {
                self.advance();
                if self.at(&Token::Dot) {
                    // err.name — error reference
                    self.advance();
                    let name = self.expect_ident();
                    Expr::ErrRef(name)
                } else {
                    // bare err — used as variable (from let x, err = ...)
                    Expr::Ident("err".to_string())
                }
            }
            Token::Ok => {
                self.advance();
                Expr::Ident("Ok".to_string())
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
                        let field_name = self.expect_ident();
                        self.expect(&Token::Colon);
                        let value = self.parse_expr();
                        fields.push((field_name, value));
                        self.eat(&Token::Comma);
                    }
                    self.expect(&Token::RBrace);
                    Expr::StructLit { name, fields }
                } else {
                    Expr::Ident(name)
                }
            }
            Token::LBracket => {
                // Array literal: [1, 2, 3]
                self.advance();
                let mut elements = Vec::new();
                if !self.at(&Token::RBracket) {
                    elements.push(self.parse_expr());
                    while self.eat(&Token::Comma) {
                        if self.at(&Token::RBracket) { break; }
                        elements.push(self.parse_expr());
                    }
                }
                self.expect(&Token::RBracket);
                Expr::Array(elements)
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr();
                self.expect(&Token::RParen);
                expr
            }
            Token::Not => {
                self.advance();
                let expr = self.parse_unary();
                Expr::BinOp {
                    left: Box::new(expr),
                    op: BinOp::Eq,
                    right: Box::new(Expr::Bool(false)),
                }
            }
            Token::Match => {
                self.advance();
                let value = self.parse_expr();
                self.expect(&Token::LBrace);
                let mut arms = Vec::new();
                while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                    if self.peek() == &Token::Ident("_".to_string()) {
                        // Default arm: _ => expr
                        self.advance();
                        self.expect(&Token::FatArrow);
                        let result = self.parse_expr();
                        arms.push(MatchArm { pattern: None, value: result });
                    } else {
                        let pattern = self.parse_expr();
                        self.expect(&Token::FatArrow);
                        let result = self.parse_expr();
                        arms.push(MatchArm { pattern: Some(pattern), value: result });
                    }
                }
                self.expect(&Token::RBrace);
                Expr::Match {
                    value: Box::new(value),
                    arms,
                }
            }
            other => panic!("unexpected token in expression: {:?}", other),
        }
    }

    fn parse_args(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        if !self.at(&Token::RParen) {
            args.push(self.parse_expr());
            while self.eat(&Token::Comma) {
                args.push(self.parse_expr());
            }
        }
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::tokenize;

    #[test]
    fn parse_simple_string() {
        let mut p = Parser::new(tokenize("\"hello\""));
        assert_eq!(p.parse_expr(), Expr::String("hello".into()));
    }

    #[test]
    fn parse_binop() {
        let mut p = Parser::new(tokenize("1 + 2"));
        let expr = p.parse_expr();
        assert!(matches!(expr, Expr::BinOp { op: BinOp::Add, .. }));
    }

    #[test]
    fn parse_field_access() {
        let mut p = Parser::new(tokenize("user.name"));
        let expr = p.parse_expr();
        assert!(matches!(expr, Expr::FieldAccess { field, .. } if field == "name"));
    }

    #[test]
    fn parse_method_call() {
        let mut p = Parser::new(tokenize("name.trim()"));
        let expr = p.parse_expr();
        assert!(matches!(expr, Expr::Call { .. }));
    }

    #[test]
    fn parse_err_ref() {
        let mut p = Parser::new(tokenize("err.timeout"));
        let expr = p.parse_expr();
        assert_eq!(expr, Expr::ErrRef("timeout".into()));
    }

    #[test]
    fn parse_struct_literal() {
        let mut p = Parser::new(tokenize("Email { value: \"test\" }"));
        let expr = p.parse_expr();
        assert!(matches!(expr, Expr::StructLit { name, .. } if name == "Email"));
    }

    #[test]
    fn parse_function_call() {
        let mut p = Parser::new(tokenize("greet(\"cam\")"));
        let expr = p.parse_expr();
        assert!(matches!(expr, Expr::Call { .. }));
    }

    #[test]
    fn parse_chained_method() {
        let mut p = Parser::new(tokenize("raw.trim().to_upper()"));
        let expr = p.parse_expr();
        // Should be Call(FieldAccess(Call(FieldAccess(Ident(raw), trim)), to_upper))
        assert!(matches!(expr, Expr::Call { .. }));
    }
}
