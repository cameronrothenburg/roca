//! Rule: missing-doc
//! Requires doc comments on public functions, contracts, and structs.

use crate::ast::*;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::ItemContext;

pub struct DocsRule;

impl Rule for DocsRule {
    fn name(&self) -> &'static str { "docs" }

    fn check_item(&self, ctx: &ItemContext) -> Vec<RuleError> {
        let mut errs = Vec::new();

        match ctx.item {
            Item::Function(f) if f.is_pub && f.doc.is_none() => {
                errs.push(RuleError {
                    code: errors::MISSING_DOC.to_string(),
                    message: format!("pub fn `{}` is missing a doc comment (///)", f.name),
                    context: None,
                });
            }
            Item::Struct(s) if s.is_pub && s.doc.is_none() => {
                errs.push(RuleError {
                    code: errors::MISSING_DOC.to_string(),
                    message: format!("pub struct `{}` is missing a doc comment (///)", s.name),
                    context: None,
                });
            }
            Item::Contract(c) if c.is_pub && c.doc.is_none() => {
                errs.push(RuleError {
                    code: errors::MISSING_DOC.to_string(),
                    message: format!("pub contract `{}` is missing a doc comment (///)", c.name),
                    context: None,
                });
            }
            Item::ExternContract(c) if c.is_pub && c.doc.is_none() => {
                errs.push(RuleError {
                    code: errors::MISSING_DOC.to_string(),
                    message: format!("pub extern contract `{}` is missing a doc comment (///)", c.name),
                    context: None,
                });
            }
            Item::ExternFn(f) if f.doc.is_none() => {
                // extern fn is always pub-facing
                errs.push(RuleError {
                    code: errors::MISSING_DOC.to_string(),
                    message: format!("extern fn `{}` is missing a doc comment (///)", f.name),
                    context: None,
                });
            }
            _ => {}
        }

        errs
    }
}

#[cfg(test)]
mod tests {
    use crate::check;

    fn errors(src: &str) -> Vec<crate::errors::RuleError> {
        check::check(&crate::parse::parse(src))
    }

    fn has_code(errs: &[crate::errors::RuleError], code: &str) -> bool {
        errs.iter().any(|e| e.code == code)
    }

    #[test]
    fn pub_fn_without_doc() {
        let errs = errors("pub fn greet(name: String) -> String { return name test { self(\"a\") == \"a\" } }");
        assert!(has_code(&errs, "missing-doc"), "expected missing-doc for pub fn without ///");
    }

    #[test]
    fn pub_fn_with_doc() {
        let errs = errors("/// Greets a person\npub fn greet(name: String) -> String { return name test { self(\"a\") == \"a\" } }");
        assert!(!has_code(&errs, "missing-doc"), "should not flag pub fn with doc");
    }

    #[test]
    fn private_fn_without_doc() {
        let errs = errors("fn greet(name: String) -> String { return name test { self(\"a\") == \"a\" } }");
        assert!(!has_code(&errs, "missing-doc"), "private fn should not require doc");
    }

    #[test]
    fn pub_struct_without_doc() {
        let errs = errors(r#"pub struct P { amount: Number }{ }"#);
        assert!(has_code(&errs, "missing-doc"));
    }

    #[test]
    fn pub_struct_with_doc() {
        let errs = errors("/// A price\npub struct P { amount: Number }{ }");
        assert!(!has_code(&errs, "missing-doc"));
    }

    #[test]
    fn private_struct_no_doc_ok() {
        let errs = errors("struct P { amount: Number }{ }");
        assert!(!has_code(&errs, "missing-doc"));
    }

    #[test]
    fn pub_contract_without_doc() {
        let errs = errors("pub contract Stringable { to_string() -> String }");
        assert!(has_code(&errs, "missing-doc"));
    }

    #[test]
    fn pub_contract_with_doc() {
        let errs = errors("/// Makes things stringable\npub contract Stringable { to_string() -> String }");
        assert!(!has_code(&errs, "missing-doc"));
    }

    #[test]
    fn private_contract_no_doc_ok() {
        let errs = errors("contract Stringable { to_string() -> String }");
        assert!(!has_code(&errs, "missing-doc"));
    }
}
