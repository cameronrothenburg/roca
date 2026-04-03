//! Rule: invalid-constraint, missing-default
//! Validates field constraints on contracts and structs.

use roca_ast::*;
use roca_errors as errors;
use roca_errors::RuleError;
use crate::rule::Rule;
use crate::context::ItemContext;

pub struct ConstraintsRule;

impl Rule for ConstraintsRule {
    fn name(&self) -> &'static str { "constraints" }

    fn check_item(&self, ctx: &ItemContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        match ctx.item {
            Item::Contract(c) => check_fields(&c.fields, &c.name, &mut errors),
            Item::Struct(s) => check_fields(&s.fields, &s.name, &mut errors),
            _ => {}
        }
        errors
    }
}

// ─── Tests ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    

    fn errors(src: &str) -> Vec<roca_errors::RuleError> {
        crate::check(&roca_parse::parse(src))
    }

    #[test]
    fn valid_constraints_with_default() {
        let e = errors(r#"pub struct U { name: String { min: 1, max: 64, default: "unknown" } email: String { contains: "@", default: "none@none.com" } }{}"#);
        assert!(!e.iter().any(|e| e.code == "invalid-constraint"), "got: {:?}", e);
        assert!(!e.iter().any(|e| e.code == "missing-default"), "got: {:?}", e);
    }

    #[test]
    fn missing_default_on_constrained_field() {
        let e = errors(r#"pub struct U { name: String { min: 1, max: 64 } }{}"#);
        assert!(e.iter().any(|e| e.code == "missing-default"),
            "expected missing-default, got: {:?}", e);
    }

    #[test]
    fn default_alone_no_warning() {
        let e = errors(r#"pub struct U { name: String { default: "anon" } }{}"#);
        assert!(!e.iter().any(|e| e.code == "missing-default"),
            "default without constraints should be fine, got: {:?}", e);
    }

    #[test]
    fn min_greater_than_max() {
        assert!(errors(r#"pub struct B { age: Number { min: 150, max: 0 } }{}"#).iter().any(|e| e.code == "invalid-constraint"));
    }

    #[test]
    fn contains_on_number() {
        assert!(errors(r#"pub struct B { n: Number { contains: "x" } }{}"#).iter().any(|e| e.code == "invalid-constraint"));
    }

    #[test]
    fn bool_constraint_rejected() {
        assert!(errors(r#"pub struct B { active: Bool { min: 0 } }{}"#).iter().any(|e| e.code == "invalid-constraint"));
    }

    #[test]
    fn pattern_on_number_rejected() {
        assert!(errors(r#"pub struct B { n: Number { pattern: "[0-9]" } }{}"#).iter().any(|e| e.code == "invalid-constraint"));
    }

    #[test]
    fn valid_number_min_max() {
        assert!(errors(r#"pub struct B { n: Number { min: 0, max: 100 } }{}"#).iter().all(|e| e.code != "invalid-constraint"));
    }

    #[test]
    fn multiple_constraints_valid() {
        assert!(errors(r#"pub struct B { name: String { min: 1, max: 64, contains: "@" } }{}"#).iter().all(|e| e.code != "invalid-constraint"));
    }
}

fn check_fields(fields: &[Field], parent: &str, errors: &mut Vec<RuleError>) {
    for field in fields {
        let ctx = format!("{}.{}", parent, field.name);
        for constraint in &field.constraints {
            match (&field.type_ref, constraint) {
                (TypeRef::Number, Constraint::Contains(_)) | (TypeRef::Number, Constraint::Pattern(_)) => {
                    errors.push(RuleError::new(errors::INVALID_CONSTRAINT, format!("cannot use contains/pattern on Number field '{}'", field.name), Some(ctx.clone())));
                }
                (TypeRef::Bool, _) => {
                    errors.push(RuleError::new(errors::INVALID_CONSTRAINT, format!("Bool field '{}' cannot have constraints", field.name), Some(ctx.clone())));
                }
                _ => {}
            }
        }
        let mut min_val = None;
        let mut max_val = None;
        let has_validation = field.constraints.iter().any(|c| !matches!(c, Constraint::Default(_)));
        let has_default = field.constraints.iter().any(|c| matches!(c, Constraint::Default(_)));
        for c in &field.constraints {
            match c {
                Constraint::Min(n) | Constraint::MinLen(n) => min_val = Some(*n),
                Constraint::Max(n) | Constraint::MaxLen(n) => max_val = Some(*n),
                _ => {}
            }
        }
        if let (Some(min), Some(max)) = (min_val, max_val) {
            if min > max {
                errors.push(RuleError::new(errors::INVALID_CONSTRAINT, format!("min ({}) > max ({}) on field '{}'", min, max, field.name), Some(ctx.clone())));
            }
        }
        if has_validation && !has_default {
            errors.push(RuleError::new(errors::MISSING_DEFAULT, format!("field '{}' has constraints but no default — add default: \"value\"", field.name), Some(ctx)));
        }
    }
}
