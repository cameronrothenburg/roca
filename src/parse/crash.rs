use crate::ast::*;
use super::expr::Parser;
use super::tokenizer::Token;

impl Parser {
    /// Parse: crash { handlers }
    pub fn parse_crash_block(&mut self) -> CrashBlock {
        self.expect(&Token::Crash);
        self.expect(&Token::LBrace);

        let mut handlers = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            handlers.push(self.parse_crash_handler());
        }
        self.expect(&Token::RBrace);

        CrashBlock { handlers }
    }

    /// Parse a single crash handler:
    ///   call -> strategy
    ///   call { err.name -> strategy, default -> strategy }
    fn parse_crash_handler(&mut self) -> CrashHandler {
        // Parse the call reference (e.g. "http.get", "Email.validate", "name.trim")
        let mut call = self.expect_ident();
        while self.eat(&Token::Dot) {
            let part = self.expect_ident();
            call = format!("{}.{}", call, part);
        }

        if self.eat(&Token::Arrow) {
            // Simple: call -> strategy
            let strategy = self.parse_crash_strategy();
            CrashHandler {
                call,
                strategy: CrashHandlerKind::Simple(strategy),
            }
        } else if self.at(&Token::LBrace) {
            // Detailed: call { err.name -> strategy, ... }
            self.advance();
            let mut arms = Vec::new();
            let mut default = None;

            while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                if self.at(&Token::Default) {
                    self.advance();
                    self.expect(&Token::Arrow);
                    default = Some(self.parse_crash_strategy());
                } else if self.at(&Token::Err) {
                    self.advance();
                    self.expect(&Token::Dot);
                    let err_name = self.expect_ident();
                    self.expect(&Token::Arrow);
                    let strategy = self.parse_crash_strategy();
                    arms.push(CrashArm { err_name, strategy });
                } else {
                    panic!("expected err.name or default in crash handler, got {:?}", self.peek());
                }
            }
            self.expect(&Token::RBrace);

            CrashHandler {
                call,
                strategy: CrashHandlerKind::Detailed { arms, default },
            }
        } else {
            panic!("expected -> or {{ after crash call, got {:?}", self.peek());
        }
    }

    /// Parse a crash strategy: retry(n, ms), skip, halt, fallback(val)
    fn parse_crash_strategy(&mut self) -> CrashStrategy {
        match self.peek().clone() {
            Token::Retry => {
                self.advance();
                self.expect(&Token::LParen);
                let attempts = match self.advance() {
                    Token::NumberLit(n) => n as u32,
                    other => panic!("expected number for retry attempts, got {:?}", other),
                };
                self.expect(&Token::Comma);
                let delay = match self.advance() {
                    Token::NumberLit(n) => n as u32,
                    other => panic!("expected number for retry delay, got {:?}", other),
                };
                self.expect(&Token::RParen);
                CrashStrategy::Retry {
                    attempts,
                    delay_ms: delay,
                }
            }
            Token::Skip => {
                self.advance();
                CrashStrategy::Skip
            }
            Token::Halt => {
                self.advance();
                CrashStrategy::Halt
            }
            Token::Fallback => {
                self.advance();
                self.expect(&Token::LParen);
                let value = self.parse_expr();
                self.expect(&Token::RParen);
                CrashStrategy::Fallback(value)
            }
            other => panic!("expected crash strategy (retry/skip/halt/fallback), got {:?}", other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::tokenize;

    #[test]
    fn parse_simple_crash() {
        let mut p = Parser::new(tokenize("crash { name.trim -> halt }"));
        let c = p.parse_crash_block();
        assert_eq!(c.handlers.len(), 1);
        assert_eq!(c.handlers[0].call, "name.trim");
        assert!(matches!(c.handlers[0].strategy, CrashHandlerKind::Simple(CrashStrategy::Halt)));
    }

    #[test]
    fn parse_retry_strategy() {
        let mut p = Parser::new(tokenize("crash { db.save -> retry(3, 1000) }"));
        let c = p.parse_crash_block();
        assert!(matches!(
            c.handlers[0].strategy,
            CrashHandlerKind::Simple(CrashStrategy::Retry { attempts: 3, delay_ms: 1000 })
        ));
    }

    #[test]
    fn parse_detailed_crash() {
        let src = r#"crash {
            http.get {
                err.timeout -> retry(3, 1000)
                err.not_found -> fallback("empty")
                default -> halt
            }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let c = p.parse_crash_block();
        assert_eq!(c.handlers[0].call, "http.get");
        if let CrashHandlerKind::Detailed { arms, default } = &c.handlers[0].strategy {
            assert_eq!(arms.len(), 2);
            assert_eq!(arms[0].err_name, "timeout");
            assert!(default.is_some());
        } else {
            panic!("expected detailed handler");
        }
    }
}
