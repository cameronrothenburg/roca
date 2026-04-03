//! Recursive descent parser for the Roca language.
//!
//! Produces roca_lang AST nodes from a token stream.

use crate::tokenizer::{tokenize, Token};
use roca_lang::*;

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek2(&self) -> Option<&Token> {
        self.tokens.get(self.pos + 1)
    }

    fn advance(&mut self) -> &Token {
        let t = &self.tokens[self.pos];
        self.pos += 1;
        t
    }

    fn expect(&mut self, expected: &Token) {
        let t = self.advance().clone();
        if !tokens_match(&t, expected) {
            panic!("expected {:?} but got {:?}", expected, t);
        }
    }

    fn expect_ident(&mut self) -> String {
        match self.advance().clone() {
            Token::Ident(s) => s,
            Token::O => "o".into(),
            Token::B => "b".into(),
            t => panic!("expected identifier, got {:?}", t),
        }
    }

    fn check(&self, tok: &Token) -> bool {
        tokens_match(self.peek(), tok)
    }

    fn eat(&mut self, tok: &Token) -> bool {
        if tokens_match(self.peek(), tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    // ─── Top-level ──────────────────────────────────────

    fn parse_file(&mut self) -> SourceFile {
        let mut items = Vec::new();
        while !self.check(&Token::Eof) {
            items.push(self.parse_item());
        }
        SourceFile { items }
    }

    fn parse_item(&mut self) -> Item {
        let is_pub = self.eat(&Token::Pub);

        match self.peek().clone() {
            Token::Fn => {
                let func = self.parse_funcdef(is_pub);
                Item::Function(func)
            }
            Token::Struct => {
                let s = self.parse_structdef(is_pub);
                Item::Struct(s)
            }
            Token::Enum => {
                let e = self.parse_enumdef(is_pub);
                Item::Enum(e)
            }
            Token::Import => {
                assert!(!is_pub, "import cannot be pub");
                self.parse_import()
            }
            t => panic!("unexpected token at item level: {:?}", t),
        }
    }

    // ─── Import ─────────────────────────────────────────

    fn parse_import(&mut self) -> Item {
        self.expect(&Token::Import);
        self.expect(&Token::LBrace);
        let mut names = Vec::new();
        while !self.check(&Token::RBrace) {
            names.push(self.expect_ident());
            self.eat(&Token::Comma);
        }
        self.expect(&Token::RBrace);
        self.expect(&Token::From);
        let path = match self.advance().clone() {
            Token::String(s) => s,
            t => panic!("expected string path, got {:?}", t),
        };
        Item::Import { names, path }
    }

    // ─── Function ───────────────────────────────────────

    fn parse_funcdef(&mut self, is_pub: bool) -> FuncDef {
        self.expect(&Token::Fn);
        let name = self.expect_ident();

        // Params
        self.expect(&Token::LParen);
        let mut params = Vec::new();
        while !self.check(&Token::RParen) {
            params.push(self.parse_param());
            self.eat(&Token::Comma);
        }
        self.expect(&Token::RParen);

        // Return type
        self.expect(&Token::Arrow);
        let ret = self.parse_type();

        // Body
        self.expect(&Token::LBrace);
        let (body, test) = self.parse_body();
        self.expect(&Token::RBrace);

        FuncDef { name, is_pub, params, ret, body, test, doc: None }
    }

    fn parse_param(&mut self) -> Param {
        let own = match self.peek().clone() {
            Token::O => { self.advance(); Some(Own::O) }
            Token::B => { self.advance(); Some(Own::B) }
            _ => None,
        };
        let name = self.expect_ident();
        self.expect(&Token::Colon);
        let ty = self.parse_type();
        Param { own, name, ty }
    }

    // Parses body stmts, stops when it sees `test` or `}`.
    // Returns (body, optional test block).
    fn parse_body(&mut self) -> (Vec<Stmt>, Option<TestBlock>) {
        let mut body = Vec::new();
        let mut test = None;

        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            if self.check(&Token::Test) {
                test = Some(self.parse_test_block());
                break;
            }
            body.push(self.parse_stmt());
        }

        (body, test)
    }

    // ─── Struct ─────────────────────────────────────────

    fn parse_structdef(&mut self, is_pub: bool) -> StructDef {
        self.expect(&Token::Struct);
        let name = self.expect_ident();

        // Fields block
        self.expect(&Token::LBrace);
        let mut fields = Vec::new();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            let field_name = self.expect_ident();
            self.expect(&Token::Colon);
            let ty = self.parse_type();
            fields.push(Field { name: field_name, ty });
        }
        self.expect(&Token::RBrace);

        // Methods block
        self.expect(&Token::LBrace);
        let mut methods = Vec::new();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            let method_pub = self.eat(&Token::Pub);
            methods.push(self.parse_funcdef(method_pub));
        }
        self.expect(&Token::RBrace);

        StructDef { name, is_pub, fields, methods, doc: None }
    }

    // ─── Enum ───────────────────────────────────────────

    fn parse_enumdef(&mut self, is_pub: bool) -> EnumDef {
        self.expect(&Token::Enum);
        let name = self.expect_ident();
        self.expect(&Token::LBrace);
        let mut variants = Vec::new();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            let vname = self.expect_ident();
            if self.check(&Token::LParen) {
                self.advance();
                let mut types = Vec::new();
                while !self.check(&Token::RParen) {
                    types.push(self.parse_type());
                    self.eat(&Token::Comma);
                }
                self.expect(&Token::RParen);
                variants.push(Variant::Data(vname, types));
            } else {
                variants.push(Variant::Unit(vname));
            }
            self.eat(&Token::Comma);
        }
        self.expect(&Token::RBrace);
        EnumDef { name, is_pub, variants, doc: None }
    }

    // ─── Test block ─────────────────────────────────────

    fn parse_test_block(&mut self) -> TestBlock {
        self.expect(&Token::Test);
        self.expect(&Token::LBrace);
        let mut cases = Vec::new();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            // self(args...) == expected
            self.expect(&Token::Self_);
            self.expect(&Token::LParen);
            let mut args = Vec::new();
            while !self.check(&Token::RParen) {
                args.push(self.parse_expr());
                self.eat(&Token::Comma);
            }
            self.expect(&Token::RParen);
            self.expect(&Token::Eq);
            let expected = self.parse_expr();
            cases.push(TestCase::Equals { args, expected });
        }
        self.expect(&Token::RBrace);
        TestBlock { cases }
    }

    // ─── Types ──────────────────────────────────────────

    fn parse_type(&mut self) -> Type {
        match self.advance().clone() {
            Token::Ident(s) => match s.as_str() {
                "Int" => Type::Int,
                "Float" => Type::Float,
                "String" => Type::String,
                "Bool" => Type::Bool,
                "Array" => {
                    // Array or Array<T>
                    if self.check(&Token::Lt) {
                        self.advance();
                        let inner = self.parse_type();
                        self.expect(&Token::Gt);
                        Type::Array(Box::new(inner))
                    } else {
                        Type::Array(Box::new(Type::Named("Any".into())))
                    }
                }
                "Optional" => {
                    self.expect(&Token::Lt);
                    let inner = self.parse_type();
                    self.expect(&Token::Gt);
                    Type::Optional(Box::new(inner))
                }
                _ => Type::Named(s),
            },
            Token::Unit => Type::Unit,
            Token::Fn => {
                // fn(Types) -> RetType
                self.expect(&Token::LParen);
                let mut param_types = Vec::new();
                while !self.check(&Token::RParen) {
                    param_types.push(self.parse_type());
                    self.eat(&Token::Comma);
                }
                self.expect(&Token::RParen);
                self.expect(&Token::Arrow);
                let ret = self.parse_type();
                Type::Fn(param_types, Box::new(ret))
            }
            t => panic!("expected type, got {:?}", t),
        }
    }

    // ─── Statements ─────────────────────────────────────

    fn parse_stmt(&mut self) -> Stmt {
        match self.peek().clone() {
            // `const x = expr` → Stmt::Let (is_const = true)
            Token::Const => {
                self.advance();
                let name = self.expect_ident();
                self.expect(&Token::Assign);
                let value = self.parse_expr();
                Stmt::Let { name, ty: None, value, is_const: true }
            }

            // `var x = expr` → Stmt::Var
            Token::Var => {
                self.advance();
                let name = self.expect_ident();
                self.expect(&Token::Assign);
                let value = self.parse_expr();
                Stmt::Var { name, ty: None, value }
            }

            // `let x, err = expr` OR `let x = expr` → Stmt::Let (is_const = false)
            Token::Let => {
                self.advance();
                let name = self.expect_ident();
                // Consume optional `, err` or other comma-separated names (ignored)
                while self.eat(&Token::Comma) {
                    self.expect_ident(); // consume the extra name (e.g. `err`)
                }
                self.expect(&Token::Assign);
                let value = self.parse_expr();
                Stmt::Let { name, ty: None, value, is_const: false }
            }

            Token::Return => {
                self.advance();
                let value = self.parse_expr();
                Stmt::Return(value)
            }

            Token::If => {
                self.parse_if_stmt()
            }

            Token::For => {
                self.advance();
                let name = self.expect_ident();
                self.expect(&Token::In);
                let iter = self.parse_expr();
                self.expect(&Token::LBrace);
                let mut body = Vec::new();
                while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                    body.push(self.parse_stmt());
                }
                self.expect(&Token::RBrace);
                Stmt::For { name, iter, body }
            }

            Token::Loop => {
                self.advance();
                self.expect(&Token::LBrace);
                let mut body = Vec::new();
                while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                    body.push(self.parse_stmt());
                }
                self.expect(&Token::RBrace);
                Stmt::Loop { body }
            }

            Token::Break => { self.advance(); Stmt::Break }
            Token::Continue => { self.advance(); Stmt::Continue }

            // self.field = value  OR  self(...)  OR  expr stmt
            Token::Self_ => {
                self.parse_self_stmt()
            }

            // ident = value  OR  ident.field = value  OR  expr stmt
            Token::Ident(_) => {
                self.parse_assign_or_expr_stmt()
            }

            _ => {
                let expr = self.parse_expr();
                Stmt::Expr(expr)
            }
        }
    }

    fn parse_if_stmt(&mut self) -> Stmt {
        self.expect(&Token::If);
        let cond = self.parse_expr();
        self.expect(&Token::LBrace);
        let mut then = Vec::new();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            then.push(self.parse_stmt());
        }
        self.expect(&Token::RBrace);
        let else_ = if self.eat(&Token::Else) {
            self.expect(&Token::LBrace);
            let mut else_body = Vec::new();
            while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                else_body.push(self.parse_stmt());
            }
            self.expect(&Token::RBrace);
            Some(else_body)
        } else {
            None
        };
        Stmt::If { cond, then, else_ }
    }

    fn parse_self_stmt(&mut self) -> Stmt {
        // Consume `self`
        self.advance();

        if self.check(&Token::Dot) {
            // self.field ... could be set or expr
            self.advance(); // consume `.`
            let field = self.expect_ident();

            if self.check(&Token::Assign) {
                // self.field = value
                self.advance();
                let value = self.parse_expr();
                Stmt::SetField { target: Expr::SelfRef, field, value }
            } else {
                // self.field(...) as expression stmt — build expr and continue
                let base = Expr::GetField { target: Box::new(Expr::SelfRef), field };
                let expr = self.parse_postfix(base);
                Stmt::Expr(expr)
            }
        } else {
            // self(...) — call as expression
            let expr = self.parse_postfix(Expr::SelfRef);
            Stmt::Expr(expr)
        }
    }

    fn parse_assign_or_expr_stmt(&mut self) -> Stmt {
        // We need lookahead to distinguish:
        //   ident = expr         → Stmt::Assign
        //   ident.field = expr   → Stmt::SetField
        //   ident[idx] = expr    → Stmt::ArraySet
        //   anything else        → Stmt::Expr

        // Look ahead: ident followed by `=` (but not `==`)
        if let Token::Ident(name) = self.peek().clone() {
            if let Some(next) = self.peek2() {
                if tokens_match(next, &Token::Assign) {
                    // ident = expr
                    self.advance(); // name
                    self.advance(); // =
                    let value = self.parse_expr();
                    return Stmt::Assign { target: name, value };
                }
            }
        }

        // Otherwise parse as expression statement; may detect SetField by postfix
        let expr = self.parse_expr();

        // We don't need to detect assignment after parsing here — struct method
        // self.count = ... is handled in parse_self_stmt.
        Stmt::Expr(expr)
    }

    // ─── Expressions (recursive descent with precedence) ─

    fn parse_expr(&mut self) -> Expr {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Expr {
        let mut left = self.parse_and();
        while self.check(&Token::Or) {
            self.advance();
            let right = self.parse_and();
            left = Expr::BinOp { op: BinOp::Or, left: Box::new(left), right: Box::new(right) };
        }
        left
    }

    fn parse_and(&mut self) -> Expr {
        let mut left = self.parse_eq();
        while self.check(&Token::And) {
            self.advance();
            let right = self.parse_eq();
            left = Expr::BinOp { op: BinOp::And, left: Box::new(left), right: Box::new(right) };
        }
        left
    }

    fn parse_eq(&mut self) -> Expr {
        let mut left = self.parse_cmp();
        loop {
            let op = match self.peek() {
                Token::Eq => BinOp::Eq,
                Token::Ne => BinOp::Ne,
                _ => break,
            };
            self.advance();
            let right = self.parse_cmp();
            left = Expr::BinOp { op, left: Box::new(left), right: Box::new(right) };
        }
        left
    }

    fn parse_cmp(&mut self) -> Expr {
        let mut left = self.parse_add();
        loop {
            let op = match self.peek() {
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::Le => BinOp::Le,
                Token::Ge => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.parse_add();
            left = Expr::BinOp { op, left: Box::new(left), right: Box::new(right) };
        }
        left
    }

    fn parse_add(&mut self) -> Expr {
        let mut left = self.parse_mul();
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_mul();
            left = Expr::BinOp { op, left: Box::new(left), right: Box::new(right) };
        }
        left
    }

    fn parse_mul(&mut self) -> Expr {
        let mut left = self.parse_unary();
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary();
            left = Expr::BinOp { op, left: Box::new(left), right: Box::new(right) };
        }
        left
    }

    fn parse_unary(&mut self) -> Expr {
        match self.peek().clone() {
            Token::Not => {
                self.advance();
                let expr = self.parse_unary();
                Expr::UnaryOp { op: UnaryOp::Not, expr: Box::new(expr) }
            }
            Token::Minus => {
                self.advance();
                let expr = self.parse_unary();
                Expr::UnaryOp { op: UnaryOp::Neg, expr: Box::new(expr) }
            }
            _ => self.parse_primary_with_postfix(),
        }
    }

    fn parse_primary_with_postfix(&mut self) -> Expr {
        let base = self.parse_primary();
        self.parse_postfix(base)
    }

    fn parse_postfix(&mut self, mut expr: Expr) -> Expr {
        loop {
            match self.peek().clone() {
                Token::Dot => {
                    self.advance();
                    let field = self.expect_ident();
                    // Check if followed by `(` — method call
                    if self.check(&Token::LParen) {
                        // This is a method call: expr.method(args)
                        self.advance(); // `(`
                        let mut args = Vec::new();
                        while !self.check(&Token::RParen) {
                            args.push(self.parse_expr());
                            self.eat(&Token::Comma);
                        }
                        self.expect(&Token::RParen);
                        let target = Expr::GetField { target: Box::new(expr), field };
                        expr = Expr::Call { target: Box::new(target), args };
                    } else {
                        expr = Expr::GetField { target: Box::new(expr), field };
                    }
                }
                Token::LParen => {
                    self.advance();
                    let mut args = Vec::new();
                    while !self.check(&Token::RParen) {
                        args.push(self.parse_expr());
                        self.eat(&Token::Comma);
                    }
                    self.expect(&Token::RParen);
                    expr = Expr::Call { target: Box::new(expr), args };
                }
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_expr();
                    self.expect(&Token::RBracket);
                    expr = Expr::ArrayGet { target: Box::new(expr), index: Box::new(index) };
                }
                _ => break,
            }
        }
        expr
    }

    fn parse_primary(&mut self) -> Expr {
        match self.peek().clone() {
            Token::Int(n) => { self.advance(); Expr::Lit(Lit::Int(n)) }
            Token::Float(f) => { self.advance(); Expr::Lit(Lit::Float(f)) }
            Token::String(s) => { self.advance(); Expr::Lit(Lit::String(s)) }
            Token::Bool(b) => { self.advance(); Expr::Lit(Lit::Bool(b)) }
            Token::Unit => { self.advance(); Expr::Lit(Lit::Unit) }

            Token::Self_ => { self.advance(); Expr::SelfRef }

            Token::Wait => {
                self.advance();
                let inner = self.parse_primary_with_postfix();
                Expr::Wait(Box::new(inner))
            }

            Token::Not => {
                self.advance();
                let expr = self.parse_primary_with_postfix();
                Expr::UnaryOp { op: UnaryOp::Not, expr: Box::new(expr) }
            }

            Token::Fn => {
                // Closure: fn(params) -> body_expr
                self.advance();
                self.expect(&Token::LParen);
                let mut params = Vec::new();
                while !self.check(&Token::RParen) {
                    params.push(self.expect_ident());
                    self.eat(&Token::Comma);
                }
                self.expect(&Token::RParen);
                self.expect(&Token::Arrow);
                let body = self.parse_expr();
                Expr::MakeClosure { params, body: Box::new(body) }
            }

            Token::If => {
                self.advance();
                let cond = self.parse_expr();
                self.expect(&Token::LBrace);
                let mut then_stmts = Vec::new();
                while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                    then_stmts.push(self.parse_stmt());
                }
                self.expect(&Token::RBrace);
                // Expr-level if — return as block expr
                // Simplification: treat as Block with condition
                let cond_expr = cond;
                let else_ = if self.eat(&Token::Else) {
                    self.expect(&Token::LBrace);
                    let mut else_stmts = Vec::new();
                    while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                        else_stmts.push(self.parse_stmt());
                    }
                    self.expect(&Token::RBrace);
                    Some(Box::new(Expr::Block(else_stmts, None)))
                } else {
                    None
                };
                Expr::If {
                    cond: Box::new(cond_expr),
                    then: Box::new(Expr::Block(then_stmts, None)),
                    else_,
                }
            }

            Token::Match => {
                self.advance();
                let value = self.parse_expr();
                self.expect(&Token::LBrace);
                let mut arms = Vec::new();
                while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                    let pattern = self.parse_pattern();
                    self.expect(&Token::FatArrow);
                    let body = self.parse_expr();
                    arms.push(MatchArm { pattern, body });
                }
                self.expect(&Token::RBrace);
                Expr::Match { value: Box::new(value), arms }
            }

            Token::LBracket => {
                // Array literal [a, b, c]
                self.advance();
                let mut elems = Vec::new();
                while !self.check(&Token::RBracket) {
                    elems.push(self.parse_expr());
                    self.eat(&Token::Comma);
                }
                self.expect(&Token::RBracket);
                Expr::ArrayNew(elems)
            }

            Token::LParen => {
                self.advance();
                let expr = self.parse_expr();
                self.expect(&Token::RParen);
                expr
            }

            Token::LBrace => {
                // Block expression
                self.advance();
                let mut stmts = Vec::new();
                while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                    stmts.push(self.parse_stmt());
                }
                self.expect(&Token::RBrace);
                Expr::Block(stmts, None)
            }

            Token::Ident(name) => {
                self.advance();
                let name_str = name;

                // StructLit: Name { field: val, ... }
                // But only if we're not in a context where `{` starts a block.
                // Heuristic: if name starts with uppercase and next is `{`, it's a struct lit.
                if self.check(&Token::LBrace) && name_str.chars().next().map_or(false, |c| c.is_uppercase()) {
                    self.advance(); // `{`
                    let mut fields = Vec::new();
                    while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                        let field_name = self.expect_ident();
                        self.expect(&Token::Colon);
                        let field_val = self.parse_expr();
                        fields.push((field_name, field_val));
                        self.eat(&Token::Comma);
                    }
                    self.expect(&Token::RBrace);
                    return Expr::StructLit { name: name_str, fields };
                }

                // EnumVariant: Name.Variant or Name.Variant(args)
                // Check: next is `.` and then Ident starting uppercase — that's an enum variant
                // (vs field access which would be lowercase)
                if self.check(&Token::Dot) {
                    if let Some(Token::Ident(next_name)) = self.peek2().cloned() {
                        if next_name.chars().next().map_or(false, |c| c.is_uppercase()) {
                            self.advance(); // `.`
                            let variant = self.expect_ident();
                            if self.check(&Token::LParen) {
                                self.advance();
                                let mut args = Vec::new();
                                while !self.check(&Token::RParen) {
                                    args.push(self.parse_expr());
                                    self.eat(&Token::Comma);
                                }
                                self.expect(&Token::RParen);
                                return Expr::EnumVariant { name: name_str, variant, args };
                            } else {
                                return Expr::EnumVariant { name: name_str, variant, args: vec![] };
                            }
                        }
                    }
                }

                Expr::Ident(name_str)
            }

            // `o` and `b` are ownership keywords but also valid variable names
            Token::O => { self.advance(); Expr::Ident("o".into()) }
            Token::B => { self.advance(); Expr::Ident("b".into()) }

            t => panic!("unexpected token in expression: {:?}", t),
        }
    }

    // ─── Patterns ───────────────────────────────────────

    fn parse_pattern(&mut self) -> Pattern {
        match self.peek().clone() {
            Token::Ident(name) if name == "_" => {
                self.advance();
                Pattern::Wildcard
            }
            Token::Ident(name) => {
                self.advance();
                // Could be Name.Variant(bindings)
                if self.check(&Token::Dot) {
                    self.advance();
                    let variant = self.expect_ident();
                    if self.check(&Token::LParen) {
                        self.advance();
                        let mut bindings = Vec::new();
                        while !self.check(&Token::RParen) {
                            bindings.push(self.expect_ident());
                            self.eat(&Token::Comma);
                        }
                        self.expect(&Token::RParen);
                        Pattern::Variant { name, variant, bindings }
                    } else {
                        Pattern::Variant { name, variant, bindings: vec![] }
                    }
                } else {
                    // Plain ident pattern — treat as wildcard binding
                    Pattern::Wildcard
                }
            }
            Token::Int(n) => {
                self.advance();
                Pattern::Lit(Lit::Int(n))
            }
            Token::String(s) => {
                self.advance();
                Pattern::Lit(Lit::String(s))
            }
            Token::Bool(b) => {
                self.advance();
                Pattern::Lit(Lit::Bool(b))
            }
            t => panic!("unexpected token in pattern: {:?}", t),
        }
    }
}

// ─── Token comparison (ignoring value for most) ─────────

fn tokens_match(a: &Token, b: &Token) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

// ─── Public API ─────────────────────────────────────────

pub fn parse(source: &str) -> SourceFile {
    let tokens = tokenize(source);
    let mut parser = Parser::new(tokens);
    parser.parse_file()
}

pub fn parse_project(files: &[(&str, &str)]) -> Vec<SourceFile> {
    files.iter().map(|(_, src)| parse(src)).collect()
}
