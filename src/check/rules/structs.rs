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
                    errors.push(RuleError {
                        code: errors::EMPTY_STRUCT.into(),
                        message: format!("struct '{}' has no fields or methods — use a contract instead", s.name),
                        context: None,
                    });
                    return errors;
                }
            }
            for sig in &s.signatures {
                if !s.methods.iter().any(|m| m.name == sig.name) {
                    errors.push(RuleError {
                        code: errors::MISSING_IMPL.into(),
                        message: format!("struct '{}' declares '{}' in contract but has no implementation", s.name, sig.name),
                        context: None,
                    });
                }
            }
            for method in &s.methods {
                match s.signatures.iter().find(|sig| sig.name == method.name) {
                    Some(sig) => {
                        if sig.params.len() != method.params.len() {
                            errors.push(RuleError {
                                code: errors::SIG_MISMATCH.into(),
                                message: format!("'{}.{}' has {} params but contract declares {}", s.name, method.name, method.params.len(), sig.params.len()),
                                context: None,
                            });
                        }
                        if sig.return_type != method.return_type {
                            errors.push(RuleError {
                                code: errors::SIG_MISMATCH.into(),
                                message: format!("'{}.{}' returns {:?} but contract declares {:?}", s.name, method.name, method.return_type, sig.return_type),
                                context: None,
                            });
                        }
                    }
                    None => {
                        errors.push(RuleError {
                            code: errors::UNDECLARED_METHOD.into(),
                            message: format!("'{}.{}' is not declared in the struct's contract block", s.name, method.name),
                            context: None,
                        });
                    }
                }
            }
        }
        errors
    }
}
