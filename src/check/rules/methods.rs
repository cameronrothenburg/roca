//! Rule: nullable-access, unknown-method, private-method, generic-mismatch,
//! constraint-violation, type-mismatch, struct-comparison, invalid-ordering, not-loggable
//! Validates method calls, field access, and type compatibility on expressions.

use std::collections::HashMap;
use crate::ast::*;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::ExprContext;
use crate::check::walker::{resolve_type, type_ref_to_name};
use crate::constants::CONSOLE_BUILTINS;

#[cfg(test)]
mod tests {
    use crate::check;

    fn errors(src: &str) -> Vec<crate::errors::RuleError> {
        check::check(&crate::parse::parse(src))
    }

    #[test]
    fn valid_method() {
        assert!(!errors(r#"pub fn p(n: String) -> String { return n.trim() crash { n.trim -> halt } test { self("a") == "a" } }"#)
            .iter().any(|e| e.code == "unknown-method"));
    }

    #[test]
    fn unknown_method() {
        assert!(errors(r#"pub fn p(n: String) -> String { return n.fake() crash { n.fake -> halt } test { self("a") == "a" } }"#)
            .iter().any(|e| e.code == "unknown-method"));
    }

    #[test]
    fn number_cant_trim() {
        assert!(errors(r#"pub fn p(n: Number) -> String { return n.trim() crash { n.trim -> halt } test { self(1) == "1" } }"#)
            .iter().any(|e| e.code == "unknown-method"));
    }

    #[test]
    fn type_mismatch_comparison() {
        assert!(errors(r#"pub fn p(n: Number, s: String) -> Bool { return n == s test { self(1, "a") == false } }"#)
            .iter().any(|e| e.code == "type-mismatch"));
    }

    #[test]
    fn bool_ordering() {
        assert!(errors(r#"pub fn p(a: Bool, b: Bool) -> Bool { return a > b test { self(true, false) == true } }"#)
            .iter().any(|e| e.code == "invalid-ordering"));
    }

    #[test]
    fn generic_push_wrong_type() {
        assert!(errors(r#"pub fn p() -> Number { const items = ["a"] items.push(42) return 0 crash { items.push -> halt } test { self() == 0 } }"#)
            .iter().any(|e| e.code == "generic-mismatch"));
    }

    #[test]
    fn generic_push_correct() {
        assert!(!errors(r#"pub fn p() -> Number { const items = ["a"] items.push("b") return 0 crash { items.push -> halt } test { self() == 0 } }"#)
            .iter().any(|e| e.code == "generic-mismatch"));
    }

    #[test]
    fn constraint_violation() {
        let e = errors(r#"contract Logger<T: Loggable> { add(item: T) -> Number } pub struct E { value: String }{} pub fn p(l: Logger<E>) -> Number { return l.add(E { value: "t" }) crash { l.add -> halt } test { self() == 0 } }"#);
        assert!(e.iter().any(|e| e.code == "constraint-violation"), "expected constraint-violation, got: {:?}", e);
    }

    #[test]
    fn constraint_satisfied() {
        let e = errors(r#"contract Logger<T: Loggable> { add(item: T) -> Number } pub fn p(l: Logger<String>) -> Number { return l.add("hi") crash { l.add -> halt } test { self() == 0 } }"#);
        assert!(!e.iter().any(|e| e.code == "constraint-violation"));
    }

    // ─── Visibility tests ─────

    #[test]
    fn private_method_blocked() {
        let e = errors(r#"
            pub struct Email {
                value: String
                validate(raw: String) -> Email
            }{
                fn validate(raw: String) -> Email {
                    return Email { value: raw }
                    test { self("a@b.com") is Ok }
                }
            }
            pub fn make(s: String) -> Email {
                return Email.validate(s)
                crash { Email.validate -> halt }
                test { self("a@b.com") is Ok }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "private-method"),
            "expected private-method, got: {:?}", e);
    }

    #[test]
    fn pub_method_allowed() {
        let e = errors(r#"
            pub struct Email {
                value: String
                validate(raw: String) -> Email
            }{
                pub fn validate(raw: String) -> Email {
                    return Email { value: raw }
                    test { self("a@b.com") is Ok }
                }
            }
            pub fn make(s: String) -> Email {
                return Email.validate(s)
                crash { Email.validate -> halt }
                test { self("a@b.com") is Ok }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "private-method"),
            "pub method should be accessible, got: {:?}", e);
    }

    #[test]
    fn private_method_allowed_inside_struct() {
        let e = errors(r#"
            pub struct Email {
                value: String
                validate(raw: String) -> Email
                helper(raw: String) -> String
            }{
                pub fn validate(raw: String) -> Email {
                    const cleaned = Email.helper(raw)
                    return Email { value: cleaned }
                    crash { Email.helper -> halt }
                    test { self("a@b.com") is Ok }
                }
                fn helper(raw: String) -> String {
                    return raw.trim()
                    crash { raw.trim -> halt }
                    test { self(" a ") == "a" }
                }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "private-method"),
            "private method should be callable within same struct, got: {:?}", e);
    }

    // ─── Type safety tests (enforced by TypeCheckRule) ─────

    #[test]
    fn return_type_mismatch_caught() {
        assert!(errors(r#"pub fn foo() -> Number { return "hello" test { self() == 0 } }"#)
            .iter().any(|e| e.code == "return-type-mismatch"));
    }

    #[test]
    fn arg_type_mismatch_caught() {
        assert!(errors(r#"pub fn greet(name: String) -> String { return name test { self("hi") == "hi" } } pub fn caller() -> String { return greet(42) crash { greet -> halt } test { self() == "hi" } }"#)
            .iter().any(|e| e.code == "arg-type-mismatch"));
    }

    #[test]
    fn constructor_field_type_mismatch_caught() {
        assert!(errors(r#"pub struct Email { value: String }{} pub fn make() -> Number { const e = Email { value: 42 } return 0 test { self() == 0 } }"#)
            .iter().any(|e| e.code == "field-type-mismatch"));
    }

    #[test]
    fn wrong_self_field_caught() {
        assert!(errors(r#"struct Point { x: Number }{ fn bad() -> Number { return self.nonexistent test { self() == 0 } } }"#)
            .iter().any(|e| e.code == "unknown-field"));
    }

    #[test]
    fn let_wrong_type_annotation_caught() {
        assert!(errors(r#"pub fn foo() -> Number { let x: Number = "hello" return x test { self() == 0 } }"#)
            .iter().any(|e| e.code == "type-annotation-mismatch"));
    }

    // ─── Adversarial: nullable access ─────

    #[test]
    fn nullable_access_caught() {
        let e = errors(r#"pub fn p(s: String | null) -> String { return s.trim() crash { s.trim -> halt } test { self("a") == "a" } }"#);
        assert!(e.iter().any(|e| e.code == "nullable-access"),
            "expected nullable-access, got: {:?}", e);
    }

    #[test]
    fn nullable_access_after_check_ok() {
        // After `if x == null { return }`, x is narrowed to non-null.
        let e = errors(r#"pub fn p(s: String | null) -> String { if s == null { return "none" } return s.trim() crash { s.trim -> halt } test { self("a") == "a" } }"#);
        assert!(!e.iter().any(|e| e.code == "nullable-access"),
            "null guard should narrow — no nullable-access expected, got: {:?}", e);
    }

    #[test]
    fn nullable_no_narrowing_without_return() {
        // A null check WITHOUT early return should NOT narrow the type.
        let e = errors(r#"pub fn p(s: String | null) -> String { if s == null { log("was null") } return s.trim() crash { s.trim -> halt } test { self("a") == "a" } }"#);
        assert!(e.iter().any(|e| e.code == "nullable-access"),
            "no early return — nullable-access should still fire, got: {:?}", e);
    }

    // ─── Adversarial: loggable ─────

    #[test]
    fn not_loggable_caught() {
        let e = errors(r#"pub fn p(items: Array<String>) -> String { log(items) return "ok" crash { } test { self(["a"]) == "ok" } }"#);
        assert!(e.iter().any(|e| e.code == "not-loggable"),
            "expected not-loggable, got: {:?}", e);
    }

    #[test]
    fn loggable_type_passes() {
        let e = errors(r#"pub fn p(s: String) -> String { log(s) return "ok" test { self("a") == "ok" } }"#);
        assert!(!e.iter().any(|e| e.code == "not-loggable"),
            "String should be loggable, got: {:?}", e);
    }

    // ─── Adversarial: struct comparison ─────

    #[test]
    fn struct_comparison_caught() {
        let e = errors(r#"pub struct E { v: String }{} pub fn p(a: E, b: E) -> Bool { return a == b test { self(E { v: "x" }, E { v: "x" }) == true } }"#);
        assert!(e.iter().any(|e| e.code == "struct-comparison"),
            "expected struct-comparison, got: {:?}", e);
    }

    #[test]
    fn same_type_comparison_ok() {
        let e = errors(r#"pub fn p(a: Number, b: Number) -> Bool { return a == b test { self(1, 1) == true } }"#);
        assert!(!e.iter().any(|e| e.code == "type-mismatch" || e.code == "struct-comparison"),
            "primitive same-type comparison should pass, got: {:?}", e);
    }

    #[test]
    fn string_ordering_ok() {
        let e = errors(r#"pub fn p(a: String, b: String) -> Bool { return a > b test { self("b", "a") == true } }"#);
        assert!(!e.iter().any(|e| e.code == "invalid-ordering" || e.code == "type-mismatch"),
            "String ordering should be valid, got: {:?}", e);
    }

    // ─── Adversarial: inferred types & chained methods ─────

    #[test]
    fn inferred_type_from_literal_checked() {
        // const x = "hello" infers String; calling a Number method on it should fail
        let e = errors(r#"pub fn p() -> String { const x = "hello" return x.toFixed(2) crash { x.toFixed -> halt } test { self() == "hello" } }"#);
        assert!(e.iter().any(|e| e.code == "unknown-method"),
            "String has no toFixed — expected unknown-method, got: {:?}", e);
    }

    #[test]
    fn chained_valid_methods_pass() {
        let e = errors(r#"pub fn p(s: String) -> String { return s.trim().toUpperCase() crash { s.trim -> halt } test { self(" a ") == "A" } }"#);
        assert!(!e.iter().any(|e| e.code == "unknown-method"),
            "trim().toUpperCase() are both valid String methods, got: {:?}", e);
    }

    // ─── Adversarial: array methods ─────

    #[test]
    fn array_method_valid() {
        let e = errors(r#"pub fn p(items: Array<String>) -> String { return items.join(",") crash { items.join -> halt } test { self(["a", "b"]) == "a,b" } }"#);
        assert!(!e.iter().any(|e| e.code == "unknown-method"),
            "Array.join is valid, got: {:?}", e);
    }

    #[test]
    fn array_method_invalid() {
        let e = errors(r#"pub fn p(items: Array<String>) -> String { return items.trim() crash { items.trim -> halt } test { self(["a"]) == "a" } }"#);
        assert!(e.iter().any(|e| e.code == "unknown-method"),
            "Array has no trim — expected unknown-method, got: {:?}", e);
    }
}

pub struct MethodsRule;

/// If type_name is a function type parameter (e.g., "T" from fn<T: Loggable>),
/// resolve it to the constraint contract name. Otherwise return as-is.
fn resolve_type_param(type_name: &str, ctx: &ExprContext) -> String {
    for tp in &ctx.func.def.type_params {
        if tp.name == type_name {
            if let Some(constraint) = &tp.constraint {
                return constraint.clone();
            }
            // Unconstrained type param — no methods available
            return type_name.to_string();
        }
    }
    type_name.to_string()
}

impl Rule for MethodsRule {
    fn name(&self) -> &'static str { "methods" }

    fn check_expr(&self, ctx: &ExprContext) -> Vec<RuleError> {
        let mut errors = Vec::new();

        match ctx.expr {
            Expr::Call { target, args } => {
                if let Expr::Ident(name) = target.as_ref() {
                    if CONSOLE_BUILTINS.contains(&name.as_str()) {
                        for arg in args {
                            check_loggable(arg, ctx, name, &mut errors);
                        }
                    }
                }

                if let Expr::FieldAccess { target: obj, field } = target.as_ref() {
                    let type_name = resolve_type(obj, ctx.scope).or_else(|| {
                        // Static calls: Email.validate() — ident is a type name, not a variable
                        if let Expr::Ident(name) = obj.as_ref() {
                            if ctx.check.registry.get(name).is_some() {
                                return Some(name.clone());
                            }
                        }
                        None
                    });
                    if let Some(type_name) = type_name {
                        // Resolve type params to their constraint contract
                        let resolved = resolve_type_param(&type_name, ctx);
                        check_method_call(&resolved, field, args, ctx, &mut errors);
                    }
                }
            }
            Expr::BinOp { left, op, right } => {
                check_binop(left, op, right, ctx, &mut errors);
            }
            _ => {}
        }

        errors
    }
}

fn check_method_call(type_name: &str, field: &str, args: &[Expr], ctx: &ExprContext, errors: &mut Vec<RuleError>) {
    let registry = ctx.check.registry;
    let qn = &ctx.func.qualified_name;

    if type_name.ends_with('?') {
        errors.push(RuleError::new(errors::NULLABLE_ACCESS, format!("cannot call .{}() on nullable type '{}'", field, type_name.trim_end_matches('?')), Some(qn.clone())));
    }

    let lookup_type = type_name.trim_end_matches('?');
    if !registry.has_method(lookup_type, field) {
        let available = registry.available_methods(lookup_type);
        let hint = if available.is_empty() { String::new() }
            else { format!("\n  available: {}", available.join(", ")) };
        errors.push(RuleError::new(errors::UNKNOWN_METHOD, format!("'{}' has no method '{}'{}",lookup_type, field, hint), Some(qn.clone())));
    } else if !registry.is_method_pub(lookup_type, field) {
        // Check if caller is inside the same struct — self calls are allowed
        let caller_struct = qn.split('.').next().unwrap_or("");
        if caller_struct != lookup_type {
            errors.push(RuleError::new(errors::PRIVATE_METHOD, format!("'{}.{}' is not pub — cannot call from outside '{}'", lookup_type, field, lookup_type), Some(qn.clone())));
        }
    }

    if lookup_type.contains('<') {
        if let Some((sig, subs)) = registry.get_method(lookup_type, field) {
            if !subs.is_empty() {
                for (i, param) in sig.params.iter().enumerate() {
                    if let Some(arg_expr) = args.get(i) {
                        let expected = substitute_type(&type_ref_to_name(&param.type_ref), &subs);
                        if let Some(actual) = resolve_type(arg_expr, ctx.scope) {
                            if !registry.type_accepts(&expected, &actual) {
                                errors.push(RuleError::new(errors::GENERIC_MISMATCH, format!("{}.{}() expects {} but got {}", lookup_type, field, expected, actual), Some(qn.clone())));
                            }
                        }
                    }
                }
            }
        }

        for (arg, constraint, full_type) in registry.check_generic_constraints(lookup_type) {
            errors.push(RuleError::new(errors::CONSTRAINT_VIOLATION, format!("'{}' does not satisfy constraint '{}' required by {}", arg, constraint, full_type), Some(qn.clone())));
        }
    }
}

fn check_binop(left: &Expr, op: &BinOp, right: &Expr, ctx: &ExprContext, errors: &mut Vec<RuleError>) {
    if !is_comparison(op) { return; }

    let left_type = resolve_type(left, ctx.scope);
    let right_type = resolve_type(right, ctx.scope);

    if let (Some(lt), Some(rt)) = (&left_type, &right_type) {
        if lt != rt {
            errors.push(RuleError::new(errors::TYPE_MISMATCH, format!("cannot compare {} with {}", lt, rt), Some(ctx.func.qualified_name.clone())));
        } else if !is_primitive(lt) {
            errors.push(RuleError::new(errors::STRUCT_COMPARISON, format!("cannot compare struct '{}' directly — compare fields instead", lt), Some(ctx.func.qualified_name.clone())));
        } else if is_ordering(op) && lt == "Bool" {
            errors.push(RuleError::new(errors::INVALID_ORDERING, "cannot order Bool — use == or != instead", Some(ctx.func.qualified_name.clone())));
        }
    }
}

fn check_loggable(expr: &Expr, ctx: &ExprContext, fn_name: &str, errors: &mut Vec<RuleError>) {
    if let Expr::Call { target, .. } = expr {
        if let Expr::FieldAccess { field, .. } = target.as_ref() {
            if field == "toLog" { return; }
        }
    }
    if let Some(type_name) = resolve_type(expr, ctx.scope) {
        if !ctx.check.registry.has_method(&type_name, "toLog") {
            errors.push(RuleError::new(errors::NOT_LOGGABLE, format!("{}() requires Loggable — '{}' has no toLog() method", fn_name, type_name), Some(ctx.func.qualified_name.clone())));
        }
    }
}

fn substitute_type(type_name: &str, substitutions: &HashMap<String, String>) -> String {
    if let Some(replacement) = substitutions.get(type_name) {
        return replacement.clone();
    }
    if let Some(lt_pos) = type_name.find('<') {
        let base = &type_name[..lt_pos];
        let args_str = &type_name[lt_pos + 1..type_name.len() - 1];
        let sub_args: Vec<String> = args_str.split(", ")
            .map(|a| substitute_type(a, substitutions))
            .collect();
        format!("{}<{}>", base, sub_args.join(", "))
    } else {
        type_name.to_string()
    }
}

fn is_comparison(op: &BinOp) -> bool {
    matches!(op, BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte)
}

fn is_ordering(op: &BinOp) -> bool {
    matches!(op, BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte)
}

fn is_primitive(type_name: &str) -> bool {
    matches!(type_name, "String" | "Number" | "Bool")
}
