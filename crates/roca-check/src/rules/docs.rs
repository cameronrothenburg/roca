//! Rule: missing-doc
//! Requires doc comments on public functions, contracts, and structs.

use roca_ast::*;
use roca_errors as errors;
use roca_errors::RuleError;
use crate::rule::Rule;
use crate::context::ItemContext;

pub struct DocsRule;

impl Rule for DocsRule {
    fn name(&self) -> &'static str { "docs" }

    fn check_item(&self, ctx: &ItemContext) -> Vec<RuleError> {
        let mut errs = Vec::new();

        match ctx.item {
            Item::Function(f) if f.is_pub && f.doc.is_none() => {
                errs.push(RuleError::new(errors::MISSING_DOC, format!("pub fn `{}` is missing a doc comment (///)", f.name), None));
            }
            Item::Struct(s) if s.is_pub && s.doc.is_none() => {
                errs.push(RuleError::new(errors::MISSING_DOC, format!("pub struct `{}` is missing a doc comment (///)", s.name), None));
            }
            Item::Contract(c) if c.is_pub && c.doc.is_none() => {
                errs.push(RuleError::new(errors::MISSING_DOC, format!("pub contract `{}` is missing a doc comment (///)", c.name), None));
            }
            Item::ExternContract(c) if c.is_pub && c.doc.is_none() => {
                errs.push(RuleError::new(errors::MISSING_DOC, format!("pub extern contract `{}` is missing a doc comment (///)", c.name), None));
            }
            Item::ExternFn(f) if f.doc.is_none() => {
                // extern fn is always pub-facing
                errs.push(RuleError::new(errors::MISSING_DOC, format!("extern fn `{}` is missing a doc comment (///)", f.name), None));
            }
            _ => {}
        }

        errs
    }
}

#[cfg(test)]
mod tests {
    

    fn errors(src: &str) -> Vec<roca_errors::RuleError> {
        crate::check(&roca_parse::parse(src))
    }

    fn has_code(errs: &[roca_errors::RuleError], code: &str) -> bool {
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
