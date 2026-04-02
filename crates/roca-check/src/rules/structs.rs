//! Rule: empty-struct, missing-impl, sig-mismatch, undeclared-method
//! Validates struct definitions, method implementations, and signature consistency.

use std::collections::{HashMap, HashSet};
use roca_ast::*;
use roca_errors as errors;
use roca_errors::RuleError;
use crate::rule::Rule;
use crate::context::ItemContext;

#[cfg(test)]
mod tests {
    

    fn errors(src: &str) -> Vec<roca_errors::RuleError> {
        crate::check(&roca_parse::parse(src))
    }

    #[test]
    fn valid_struct() {
        assert!(errors(r#"struct P { amount: Number add(other: P) -> P }{ fn add(other: P) -> P { return self test { self(self) == self } } }"#)
            .iter().all(|e| e.code != "missing-impl" && e.code != "sig-mismatch"));
    }

    #[test]
    fn missing_implementation() {
        assert!(errors(r#"struct P { amount: Number add(other: P) -> P to_string() -> String }{ fn add(other: P) -> P { return self test { self(self) == self } } }"#)
            .iter().any(|e| e.code == "missing-impl"));
    }

    #[test]
    fn undeclared_method() {
        assert!(errors(r#"struct P { amount: Number }{ fn add(other: P) -> P { return self test { self(self) == self } } }"#)
            .iter().any(|e| e.code == "undeclared-method"));
    }

    #[test]
    fn sig_mismatch_param_count() {
        // Contract declares add(other: P) with 1 param, but impl has 2 params
        assert!(errors(r#"struct P { amount: Number add(other: P) -> P }{ fn add(a: P, b: P) -> P { return a test { self(self, self) == self } } }"#)
            .iter().any(|e| e.code == "sig-mismatch"));
    }

    #[test]
    fn sig_mismatch_return_type() {
        // Contract declares -> P but impl returns -> Number
        assert!(errors(r#"struct P { amount: Number add() -> P }{ fn add() -> Number { return self.amount test { self() == 1 } } }"#)
            .iter().any(|e| e.code == "sig-mismatch"));
    }

    #[test]
    fn empty_struct_flagged() {
        let e = errors(r#"pub struct Empty {}{}"#);
        assert!(e.iter().any(|e| e.code == "empty-struct"),
            "expected empty-struct, got: {:?}", e);
    }

    #[test]
    fn data_struct_with_fields_ok() {
        let e = errors(r#"pub struct Profile { name: String }{}"#);
        assert!(!e.iter().any(|e| e.code == "empty-struct"),
            "struct with fields should be valid, got: {:?}", e);
    }

    #[test]
    fn struct_with_methods_ok() {
        let e = errors(r#"pub struct Email { value: String validate(raw: String) -> Email }{ pub fn validate(raw: String) -> Email { return Email { value: raw } test { self("a") is Ok } } }"#);
        assert!(!e.iter().any(|e| e.code == "empty-struct"),
            "struct with methods should not trigger empty-struct, got: {:?}", e);
    }

    #[test]
    fn valid_struct_multiple_methods() {
        let e = errors(r#"struct P { amount: Number add(other: P) -> P to_string() -> String }{ fn add(other: P) -> P { return self test { self(self) == self } } fn to_string() -> String { return "p" test { self() == "p" } } }"#);
        assert!(!e.iter().any(|e| e.code == "missing-impl" || e.code == "sig-mismatch" || e.code == "undeclared-method"));
    }
}

pub struct StructsRule;

impl Rule for StructsRule {
    fn name(&self) -> &'static str { "structs" }

    fn check_item(&self, ctx: &ItemContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        if let Item::Struct(s) = ctx.item {
            if s.methods.is_empty() && s.signatures.is_empty() && s.fields.is_empty() {
                // No fields, no methods, no point — use a contract
                let has_satisfies = ctx.check.file.items.iter().any(|item| {
                    matches!(item, Item::Satisfies(sat) if sat.struct_name == s.name)
                });
                if !has_satisfies {
                    errors.push(RuleError::new(errors::EMPTY_STRUCT, format!("struct '{}' has no fields or methods — use a contract instead", s.name), None));
                    return errors;
                }
            }
            // Pre-build sets/maps for O(1) cross-checks instead of O(n²) nested loops
            let method_names: HashSet<&str> = s.methods.iter().map(|m| m.name.as_str()).collect();
            let sig_by_name: HashMap<&str, &FnSignature> = s.signatures.iter()
                .map(|sig| (sig.name.as_str(), sig))
                .collect();

            for sig in &s.signatures {
                if !method_names.contains(sig.name.as_str()) {
                    errors.push(RuleError::new(errors::MISSING_IMPL, format!("struct '{}' declares '{}' in contract but has no implementation", s.name, sig.name), None));
                }
            }
            for method in &s.methods {
                match sig_by_name.get(method.name.as_str()) {
                    Some(sig) => {
                        if sig.params.len() != method.params.len() {
                            errors.push(RuleError::new(errors::SIG_MISMATCH, format!("'{}.{}' has {} params but contract declares {}", s.name, method.name, method.params.len(), sig.params.len()), None));
                        }
                        if sig.return_type != method.return_type {
                            errors.push(RuleError::new(errors::SIG_MISMATCH, format!("'{}.{}' returns {:?} but contract declares {:?}", s.name, method.name, method.return_type, sig.return_type), None));
                        }
                    }
                    None => {
                        errors.push(RuleError::new(errors::UNDECLARED_METHOD, format!("'{}.{}' is not declared in the struct's contract block", s.name, method.name), None));
                    }
                }
            }
        }
        errors
    }
}
