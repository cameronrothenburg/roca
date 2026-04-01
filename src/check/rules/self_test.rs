//! Rule: self-referential-test
//! Rejects test assertions that compare self() against self() — they prove nothing.

use crate::ast::*;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::FnCheckContext;

#[cfg(test)]
mod tests {
    use crate::check;

    fn errors(src: &str) -> Vec<crate::errors::RuleError> {
        check::check(&crate::parse::parse(src))
    }

    #[test]
    fn self_vs_self_rejected() {
        let e = errors(r#"fn hash(s: String) -> String { return s test { self("hello") == self("hello") } }"#);
        assert!(e.iter().any(|e| e.code == "self-referential-test"),
            "expected self-referential-test, got: {:?}", e);
    }

    #[test]
    fn self_vs_literal_ok() {
        let e = errors(r#"fn id(s: String) -> String { return s test { self("hello") == "hello" } }"#);
        assert!(!e.iter().any(|e| e.code == "self-referential-test"),
            "literal expected value should be fine, got: {:?}", e);
    }

    #[test]
    fn self_vs_number_ok() {
        let e = errors(r#"fn len(s: String) -> Number { return 0 test { self("hi") == 0 } }"#);
        assert!(!e.iter().any(|e| e.code == "self-referential-test"),
            "number expected value should be fine, got: {:?}", e);
    }

    #[test]
    fn is_ok_not_affected() {
        let e = errors(r#"fn v(s: String) -> String, err { if s == "" { return err.bad } return s test { self("ok") is Ok self("") is err.bad } }"#);
        assert!(!e.iter().any(|e| e.code == "self-referential-test"),
            "is Ok should not trigger self-referential-test, got: {:?}", e);
    }

    #[test]
    fn self_vs_self_no_args_rejected() {
        let e = errors(r#"fn ping() -> String { return "pong" test { self() == self() } }"#);
        assert!(e.iter().any(|e| e.code == "self-referential-test"),
            "expected self-referential-test for zero-arg self(), got: {:?}", e);
    }

    #[test]
    fn self_vs_different_args_still_rejected() {
        let e = errors(r#"fn hash(s: String) -> String { return s test { self("a") == self("b") } }"#);
        assert!(e.iter().any(|e| e.code == "self-referential-test"),
            "self() vs self() with different args is still self-referential, got: {:?}", e);
    }
}

pub struct SelfTestRule;

impl Rule for SelfTestRule {
    fn name(&self) -> &'static str { errors::SELF_REFERENTIAL_TEST }

    fn check_function(&self, ctx: &FnCheckContext) -> Vec<RuleError> {
        let mut errs = Vec::new();
        let f = ctx.func.def;

        if let Some(test) = &f.test {
            for case in &test.cases {
                if let TestCase::Equals { expected, .. } = case {
                    if matches!(expected, Expr::Call { target, .. } if matches!(**target, Expr::SelfRef)) {
                        errs.push(RuleError::new(
                            errors::SELF_REFERENTIAL_TEST,
                            "test expected value is a self() call — use a concrete expected value".to_string(),
                            Some(ctx.func.qualified_name.clone()),
                        ));
                    }
                }
            }
        }

        errs
    }
}
