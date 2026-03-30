//! Rule: unknown-contract, missing-satisfies, satisfies-mismatch
//! Validates satisfies declarations against their target contracts.

use crate::ast::*;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::ItemContext;

#[cfg(test)]
mod tests {
    use crate::check;

    fn errors(src: &str) -> Vec<crate::errors::RuleError> {
        check::check(&crate::parse::parse(src))
    }

    #[test]
    fn valid_satisfies() {
        let e = errors(r#"contract Stringable { to_string() -> String } pub struct E { value: String }{} E satisfies Stringable { fn to_string() -> String { return self.value test { self() == "t" } } }"#);
        assert!(!e.iter().any(|e| e.code == "missing-satisfies" || e.code == "unknown-contract"));
    }

    #[test]
    fn missing_method() {
        let e = errors(r#"contract S { serialize() -> String deserialize(r: String) -> String } pub struct E { value: String }{} E satisfies S { fn serialize() -> String { return self.value test { self() == "t" } } }"#);
        assert!(e.iter().any(|e| e.code == "missing-satisfies"));
    }

    #[test]
    fn unknown_contract() {
        let e = errors(r#"pub struct E { value: String }{} E satisfies Nonexistent { fn foo() -> String { return "bar" test { self() == "bar" } } }"#);
        assert!(e.iter().any(|e| e.code == "unknown-contract"));
    }

    #[test]
    fn satisfies_param_count_mismatch() {
        // Contract requires serialize() with 0 params, but impl has 1 param
        let e = errors(r#"contract S { serialize() -> String } pub struct E { value: String }{} E satisfies S { fn serialize(extra: String) -> String { return extra test { self("x") == "x" } } }"#);
        assert!(e.iter().any(|e| e.code == "satisfies-mismatch"));
    }

    #[test]
    fn satisfies_return_type_mismatch() {
        // Contract requires -> String but impl returns -> Number
        let e = errors(r#"contract S { serialize() -> String } pub struct E { value: String }{} E satisfies S { fn serialize() -> Number { return 42 test { self() == 42 } } }"#);
        assert!(e.iter().any(|e| e.code == "satisfies-mismatch"));
    }

    #[test]
    fn valid_satisfies_multiple_methods() {
        let e = errors(r#"contract S { serialize() -> String deserialize(r: String) -> String } pub struct E { value: String }{} E satisfies S { fn serialize() -> String { return self.value test { self() == "t" } } fn deserialize(r: String) -> String { return r test { self("x") == "x" } } }"#);
        assert!(!e.iter().any(|e| e.code == "missing-satisfies" || e.code == "satisfies-mismatch" || e.code == "unknown-contract"));
    }
}

pub struct SatisfiesRule;

impl Rule for SatisfiesRule {
    fn name(&self) -> &'static str { "satisfies" }

    fn check_item(&self, ctx: &ItemContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        if let Item::Satisfies(sat) = ctx.item {
            let contract = match ctx.check.registry.contracts.get(&sat.contract_name) {
                Some(c) => c,
                None => {
                    errors.push(RuleError {
                        code: errors::UNKNOWN_CONTRACT.into(),
                        message: format!("'{}' satisfies '{}' but contract '{}' does not exist", sat.struct_name, sat.contract_name, sat.contract_name),
                        context: None,
                    });
                    return errors;
                }
            };
            for sig in &contract.functions {
                match sat.methods.iter().find(|m| m.name == sig.name) {
                    None => {
                        errors.push(RuleError {
                            code: errors::MISSING_SATISFIES.into(),
                            message: format!("'{}' satisfies '{}' but does not implement '{}'", sat.struct_name, sat.contract_name, sig.name),
                            context: None,
                        });
                    }
                    Some(m) => {
                        if sig.params.len() != m.params.len() {
                            errors.push(RuleError {
                                code: errors::SATISFIES_MISMATCH.into(),
                                message: format!("'{}.{}' has {} params but '{}' requires {}", sat.struct_name, m.name, m.params.len(), sat.contract_name, sig.params.len()),
                                context: None,
                            });
                        }
                        if sig.return_type != m.return_type {
                            errors.push(RuleError {
                                code: errors::SATISFIES_MISMATCH.into(),
                                message: format!("'{}.{}' returns {:?} but '{}' requires {:?}", sat.struct_name, m.name, m.return_type, sat.contract_name, sig.return_type),
                                context: None,
                            });
                        }
                    }
                }
            }
        }
        errors
    }
}
