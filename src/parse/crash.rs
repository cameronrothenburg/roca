//! Crash block parser — error recovery handlers with chain strategies.

use crate::ast::*;
use super::expr::{Parser, ParseResult};
use super::tokenizer::Token;

impl Parser {
    pub fn parse_crash_block(&mut self) -> ParseResult<CrashBlock> {
        self.expect(&Token::Crash)?;
        self.expect(&Token::LBrace)?;

        let mut handlers = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            handlers.push(self.parse_crash_handler()?);
        }
        self.expect(&Token::RBrace)?;

        Ok(CrashBlock { handlers })
    }

    fn parse_crash_handler(&mut self) -> ParseResult<CrashHandler> {
        let mut call = match self.peek() {
            Token::SelfKw => { self.advance(); "self".to_string() }
            _ => self.expect_ident()?,
        };
        while self.eat(&Token::Dot) {
            let part = self.expect_ident()?;
            call = format!("{}.{}", call, part);
        }

        if self.eat(&Token::Arrow) {
            let chain = self.parse_crash_chain()?;
            Ok(CrashHandler {
                call,
                strategy: CrashHandlerKind::Simple(chain),
            })
        } else if self.at(&Token::LBrace) {
            self.advance();
            let mut arms = Vec::new();
            let mut default = None;

            while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                if self.at(&Token::Default) {
                    self.advance();
                    self.expect(&Token::Arrow)?;
                    default = Some(self.parse_crash_chain()?);
                } else if self.at(&Token::Err) {
                    self.advance();
                    self.expect(&Token::Dot)?;
                    let err_name = self.expect_ident()?;
                    self.expect(&Token::Arrow)?;
                    let chain = self.parse_crash_chain()?;
                    arms.push(CrashArm { err_name, chain });
                } else {
                    return Err(self.err(format!("expected err.name or default in crash handler, got {:?}", self.peek())));
                }
            }
            self.expect(&Token::RBrace)?;

            Ok(CrashHandler {
                call,
                strategy: CrashHandlerKind::Detailed { arms, default },
            })
        } else {
            Err(self.err(format!("expected -> or {{ after crash call, got {:?}", self.peek())))
        }
    }

    /// Parse a crash chain: step |> step |> step
    fn parse_crash_chain(&mut self) -> ParseResult<CrashChain> {
        let mut chain = vec![self.parse_crash_step()?];
        while self.eat(&Token::PipeArrow) {
            chain.push(self.parse_crash_step()?);
        }
        Ok(chain)
    }

    /// Parse a single crash step: retry(n, ms), skip, halt, fallback(val), log, panic
    fn parse_crash_step(&mut self) -> ParseResult<CrashStep> {
        match self.peek().clone() {
            Token::Retry => {
                self.advance();
                self.expect(&Token::LParen)?;
                let attempts = match self.advance() {
                    Token::NumberLit(n) => n as u32,
                    other => return Err(self.err(format!("expected number for retry attempts, got {:?}", other))),
                };
                self.expect(&Token::Comma)?;
                let delay = match self.advance() {
                    Token::NumberLit(n) => n as u32,
                    other => return Err(self.err(format!("expected number for retry delay, got {:?}", other))),
                };
                self.expect(&Token::RParen)?;
                Ok(CrashStep::Retry { attempts, delay_ms: delay })
            }
            Token::Skip => { self.advance(); Ok(CrashStep::Skip) }
            Token::Halt => { self.advance(); Ok(CrashStep::Halt) }
            Token::Panic => { self.advance(); Ok(CrashStep::Panic) }
            Token::Fallback => {
                self.advance();
                self.expect(&Token::LParen)?;
                let value = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(CrashStep::Fallback(value))
            }
            // log is an identifier, not a keyword (also used as console function)
            Token::Ident(ref s) if s == "log" => {
                self.advance();
                Ok(CrashStep::Log)
            }
            other => Err(self.err(format!("expected crash strategy, got {:?}", other))),
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
        let c = p.parse_crash_block().unwrap();
        assert_eq!(c.handlers[0].call, "name.trim");
        assert!(matches!(&c.handlers[0].strategy, CrashHandlerKind::Simple(chain) if chain.len() == 1));
    }

    #[test]
    fn parse_chain() {
        let mut p = Parser::new(tokenize("crash { db.save -> log |> retry(3, 1000) |> halt }"));
        let c = p.parse_crash_block().unwrap();
        if let CrashHandlerKind::Simple(chain) = &c.handlers[0].strategy {
            assert_eq!(chain.len(), 3);
            assert!(matches!(chain[0], CrashStep::Log));
            assert!(matches!(chain[1], CrashStep::Retry { attempts: 3, delay_ms: 1000 }));
            assert!(matches!(chain[2], CrashStep::Halt));
        } else {
            panic!("expected Simple chain");
        }
    }

    #[test]
    fn parse_panic() {
        let mut p = Parser::new(tokenize("crash { config.load -> panic }"));
        let c = p.parse_crash_block().unwrap();
        if let CrashHandlerKind::Simple(chain) = &c.handlers[0].strategy {
            assert!(matches!(chain[0], CrashStep::Panic));
        }
    }

    #[test]
    fn parse_log_skip() {
        let mut p = Parser::new(tokenize("crash { analytics.track -> log |> skip }"));
        let c = p.parse_crash_block().unwrap();
        if let CrashHandlerKind::Simple(chain) = &c.handlers[0].strategy {
            assert_eq!(chain.len(), 2);
            assert!(matches!(chain[0], CrashStep::Log));
            assert!(matches!(chain[1], CrashStep::Skip));
        }
    }

    #[test]
    fn parse_detailed_with_chains() {
        let src = r#"crash {
            http.get {
                err.timeout -> log |> retry(3, 1000) |> halt
                err.not_found -> fallback("empty")
                default -> log |> halt
            }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let c = p.parse_crash_block().unwrap();
        if let CrashHandlerKind::Detailed { arms, default } = &c.handlers[0].strategy {
            assert_eq!(arms[0].err_name, "timeout");
            assert_eq!(arms[0].chain.len(), 3);
            assert_eq!(arms[1].err_name, "not_found");
            assert_eq!(arms[1].chain.len(), 1);
            assert!(default.is_some());
            assert_eq!(default.as_ref().unwrap().len(), 2);
        }
    }
}
