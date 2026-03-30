use crate::ast::*;
use super::expr::{Parser, ParseResult};
use super::tokenizer::Token;

impl Parser {
    /// Parse: test { cases }
    pub fn parse_test_block(&mut self) -> ParseResult<TestBlock> {
        self.expect(&Token::Test)?;
        self.expect(&Token::LBrace)?;

        let mut cases = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            cases.push(self.parse_test_case()?);
        }
        self.expect(&Token::RBrace)?;

        Ok(TestBlock { cases })
    }

    fn parse_test_case(&mut self) -> ParseResult<TestCase> {
        // Check for status mock: StatusCode.200 { mock ... }
        if let Token::Ident(name) = self.peek().clone() {
            if self.peek_ahead(1) == &Token::Dot {
                // Could be a status mock like StatusCode.200
                let saved_pos = self.pos;
                self.advance(); // name
                self.advance(); // dot
                // Check if next is a number or ident followed by {
                match self.peek().clone() {
                    Token::NumberLit(n) => {
                        self.advance();
                        if self.at(&Token::LBrace) {
                            self.advance();
                            let status = format!("{}.{}", name, n as u32);
                            let mocks = self.parse_test_mocks()?;
                            self.expect(&Token::RBrace)?;
                            return Ok(TestCase::StatusMock { status, mocks });
                        }
                        // Not a status mock, restore
                        self.pos = saved_pos;
                    }
                    Token::Ident(val) => {
                        self.advance();
                        if self.at(&Token::LBrace) {
                            self.advance();
                            let status = format!("{}.{}", name, val);
                            let mocks = self.parse_test_mocks()?;
                            self.expect(&Token::RBrace)?;
                            return Ok(TestCase::StatusMock { status, mocks });
                        }
                        self.pos = saved_pos;
                    }
                    _ => {
                        self.pos = saved_pos;
                    }
                }
            }
        }

        // Regular test case: self(args) == expected  OR  self(args) is Ok/err.name
        self.expect(&Token::SelfKw)?;
        self.expect(&Token::LParen)?;
        let args = self.parse_test_args()?;
        self.expect(&Token::RParen)?;

        // Check for field access on result: self(args).field == expected
        // For now just handle direct comparison

        if self.eat(&Token::Is) {
            // self(args) is Ok  or  self(args) is err.name
            if self.at(&Token::Ok) {
                self.advance();
                Ok(TestCase::IsOk { args })
            } else if self.at(&Token::Err) {
                self.advance();
                self.expect(&Token::Dot)?;
                let err_name = self.expect_ident()?;
                Ok(TestCase::IsErr { args, err_name })
            } else {
                Err(self.err(format!("expected Ok or err after 'is', got {:?}", self.peek())))
            }
        } else if self.at(&Token::Eq) {
            self.advance();
            let expected = self.parse_expr()?;
            Ok(TestCase::Equals { args, expected })
        } else {
            Err(self.err(format!("expected == or is after test self(), got {:?}", self.peek())))
        }
    }

    fn parse_test_args(&mut self) -> ParseResult<Vec<Expr>> {
        let mut args = Vec::new();
        if !self.at(&Token::RParen) {
            args.push(self.parse_expr()?);
            while self.eat(&Token::Comma) {
                args.push(self.parse_expr()?);
            }
        }
        Ok(args)
    }

    fn parse_test_mocks(&mut self) -> ParseResult<Vec<TestMock>> {
        let mut mocks = Vec::new();
        while self.at(&Token::Mock) {
            self.advance();
            // target -> value
            let mut target = self.expect_ident()?;
            while self.eat(&Token::Dot) {
                let part = self.expect_ident()?;
                target = format!("{}.{}", target, part);
            }
            self.expect(&Token::Arrow)?;
            let value = self.parse_expr()?;
            mocks.push(TestMock { target, value });
        }
        Ok(mocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::tokenize;

    #[test]
    fn parse_equals_test() {
        let mut p = Parser::new(tokenize("test { self(1, 2) == 3 }"));
        let t = p.parse_test_block().unwrap();
        assert_eq!(t.cases.len(), 1);
        assert!(matches!(t.cases[0], TestCase::Equals { .. }));
    }

    #[test]
    fn parse_is_ok_test() {
        let mut p = Parser::new(tokenize("test { self(\"a@b.com\") is Ok }"));
        let t = p.parse_test_block().unwrap();
        assert!(matches!(t.cases[0], TestCase::IsOk { .. }));
    }

    #[test]
    fn parse_is_err_test() {
        let mut p = Parser::new(tokenize("test { self(\"\") is err.missing }"));
        let t = p.parse_test_block().unwrap();
        if let TestCase::IsErr { err_name, .. } = &t.cases[0] {
            assert_eq!(err_name, "missing");
        } else {
            panic!("expected IsErr");
        }
    }

    #[test]
    fn parse_multiple_cases() {
        let src = r#"test {
            self(1, 2) == 3
            self(0, 0) == 0
            self(-1, 1) == 0
        }"#;
        let mut p = Parser::new(tokenize(src));
        let t = p.parse_test_block().unwrap();
        assert_eq!(t.cases.len(), 3);
    }
}
