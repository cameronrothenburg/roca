use crate::ast::*;
use super::expr::Parser;
use super::tokenizer::Token;

impl Parser {
    /// Parse: [pub] fn name(params) -> ReturnType[, err] { body crash test }
    pub fn parse_function(&mut self, is_pub: bool) -> FnDef {
        self.expect(&Token::Fn);
        let name = self.expect_ident();
        self.expect(&Token::LParen);
        let params = self.parse_params();
        self.expect(&Token::RParen);

        // Return type
        let mut return_type = TypeRef::Ok;
        let mut returns_err = false;
        let mut errors = Vec::new();

        if self.eat(&Token::Arrow) {
            return_type = self.parse_type_ref();
            if self.eat(&Token::Comma) {
                self.expect(&Token::Err);
                returns_err = true;
            }
        }

        self.expect(&Token::LBrace);

        // Parse body statements, crash block, and test block
        let mut body = Vec::new();
        let mut crash = None;
        let mut test = None;

        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            match self.peek() {
                Token::Crash => {
                    crash = Some(self.parse_crash_block());
                }
                Token::Test => {
                    test = Some(self.parse_test_block());
                }
                _ => {
                    body.push(self.parse_stmt());
                }
            }
        }
        self.expect(&Token::RBrace);

        FnDef {
            name,
            is_pub,
            params,
            return_type,
            returns_err,
            errors,
            body,
            crash,
            test,
        }
    }

    /// Parse a statement
    pub fn parse_stmt(&mut self) -> Stmt {
        match self.peek().clone() {
            Token::Const => {
                self.advance();
                let name = self.expect_ident();
                let type_ann = if self.eat(&Token::Colon) {
                    Some(self.parse_type_ref())
                } else {
                    None
                };
                self.expect(&Token::Assign);
                let value = self.parse_expr();
                Stmt::Const { name, type_ann, value }
            }
            Token::Let => {
                self.advance();
                let name = self.expect_ident();

                // Check for destructuring: let name, err = expr OR let a, b, failed = wait all { }
                if self.eat(&Token::Comma) {
                    let mut names = vec![name.clone()];
                    // Collect all names separated by commas
                    loop {
                        let next_name = match self.advance() {
                            Token::Ident(s) => s,
                            Token::Err => "err".to_string(),
                            other => panic!("expected identifier after comma in let, got {:?}", other),
                        };
                        names.push(next_name);
                        if !self.eat(&Token::Comma) { break; }
                    }
                    self.expect(&Token::Assign);

                    // Last name is the error/failed name
                    let failed_name = names.pop().unwrap();
                    let result_names = names;

                    if self.at(&Token::Wait) || self.at(&Token::All) || self.at(&Token::First) {
                        return self.parse_wait_stmt(result_names, failed_name);
                    }

                    // Regular destructure — only supports 1 result name
                    let value = self.parse_expr();
                    return Stmt::LetResult { name: result_names.into_iter().next().unwrap_or_default(), err_name: failed_name, value };
                }

                let type_ann = if self.eat(&Token::Colon) {
                    Some(self.parse_type_ref())
                } else {
                    None
                };
                self.expect(&Token::Assign);
                let value = self.parse_expr();
                Stmt::Let { name, type_ann, value }
            }
            Token::Return => {
                self.advance();
                // Check for return err.name
                if self.at(&Token::Err) {
                    self.advance();
                    self.expect(&Token::Dot);
                    let err_name = self.expect_ident();
                    return Stmt::ReturnErr(err_name);
                }
                let value = self.parse_expr();
                Stmt::Return(value)
            }
            Token::If => {
                self.advance();
                let condition = self.parse_expr();
                self.expect(&Token::LBrace);
                let mut then_body = Vec::new();
                while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                    then_body.push(self.parse_stmt());
                }
                self.expect(&Token::RBrace);

                let else_body = if self.eat(&Token::Else) {
                    self.expect(&Token::LBrace);
                    let mut body = Vec::new();
                    while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                        body.push(self.parse_stmt());
                    }
                    self.expect(&Token::RBrace);
                    Some(body)
                } else {
                    None
                };

                Stmt::If {
                    condition,
                    then_body,
                    else_body,
                }
            }
            Token::Break => {
                self.advance();
                Stmt::Break
            }
            Token::Continue => {
                self.advance();
                Stmt::Continue
            }
            Token::While => {
                self.advance();
                let condition = self.parse_expr();
                self.expect(&Token::LBrace);
                let mut body = Vec::new();
                while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                    body.push(self.parse_stmt());
                }
                self.expect(&Token::RBrace);
                Stmt::While { condition, body }
            }
            Token::For => {
                self.advance();
                let binding = self.expect_ident();
                self.expect(&Token::In);
                let iter = self.parse_expr();
                self.expect(&Token::LBrace);
                let mut body = Vec::new();
                while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                    body.push(self.parse_stmt());
                }
                self.expect(&Token::RBrace);
                Stmt::For { binding, iter, body }
            }
            // Assignment or expression statement
            Token::Ident(name) if matches!(self.peek_ahead(1), Token::Assign) => {
                let name = self.expect_ident();
                self.expect(&Token::Assign);
                let value = self.parse_expr();
                Stmt::Assign { name, value }
            }
            _ => {
                let expr = self.parse_expr();
                Stmt::Expr(expr)
            }
        }
    }

    /// Parse: wait call() | wait all { calls } | wait first { calls }
    fn parse_wait_stmt(&mut self, names: Vec<String>, failed_name: String) -> Stmt {
        let kind = match self.peek() {
            Token::Wait => {
                self.advance();
                WaitKind::Single(self.parse_expr())
            }
            Token::All => {
                // waitAll { call1() call2() }
                self.advance();
                self.expect(&Token::LBrace);
                let mut calls = Vec::new();
                while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                    calls.push(self.parse_expr());
                }
                self.expect(&Token::RBrace);
                WaitKind::All(calls)
            }
            Token::First => {
                // waitFirst { call1() call2() }
                self.advance();
                self.expect(&Token::LBrace);
                let mut calls = Vec::new();
                while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                    calls.push(self.parse_expr());
                }
                self.expect(&Token::RBrace);
                WaitKind::First(calls)
            }
            other => panic!("expected wait, waitAll, or waitFirst, got {:?}", other),
        };

        Stmt::Wait { names, failed_name, kind }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::tokenize;

    #[test]
    fn parse_simple_function() {
        let src = r#"fn add(a: Number, b: Number) -> Number {
            return a + b
            test { self(1, 2) == 3 }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let f = p.parse_function(false);
        assert_eq!(f.name, "add");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.return_type, TypeRef::Number);
        assert!(!f.is_pub);
        assert!(f.test.is_some());
        assert_eq!(f.body.len(), 1);
    }

    #[test]
    fn parse_pub_function() {
        let src = r#"fn greet(name: String) -> String {
            return "Hello " + name
            test { self("cam") == "Hello cam" }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let f = p.parse_function(true);
        assert_eq!(f.name, "greet");
        assert!(f.is_pub);
    }

    #[test]
    fn parse_function_with_crash() {
        let src = r#"fn save(data: String, db: Database) -> Ok, err {
            db.save(data)
            return Ok

            crash {
                db.save -> retry(1, 500)
            }

            test {
                self("hello", db) is Ok
            }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let f = p.parse_function(false);
        assert!(f.crash.is_some());
        assert!(f.test.is_some());
        assert!(f.returns_err);
    }

    #[test]
    fn parse_if_statement() {
        let src = r#"fn check(x: Number) -> Bool {
            if x > 0 { return true } else { return false }
            test { self(1) == true }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let f = p.parse_function(false);
        assert!(matches!(f.body[0], Stmt::If { .. }));
    }

    #[test]
    fn parse_let_result() {
        let src = r#"fn wrap(s: String) -> Email, err {
            let e, err = Email.validate(s)
            return e
            crash { Email.validate -> halt }
            test { self("a@b.com") is Ok }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let f = p.parse_function(false);
        assert!(matches!(f.body[0], Stmt::LetResult { .. }));
    }

    #[test]
    fn parse_return_err() {
        let src = r#"fn check(s: String) -> String, err {
            if s == "" { return err.missing }
            return s
            test { self("a") == "a" self("") is err.missing }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let f = p.parse_function(false);
        if let Stmt::If { then_body, .. } = &f.body[0] {
            assert!(matches!(then_body[0], Stmt::ReturnErr(_)));
        }
    }
}
