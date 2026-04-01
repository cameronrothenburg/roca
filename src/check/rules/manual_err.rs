//! Rule: err-in-body, manual-err-check
//! Enforces that errors are handled via crash blocks, not manual destructuring.

use crate::ast::*;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::FnCheckContext;

/// Rule: errors must be handled in crash blocks only.
/// No let val, err = call() destructuring — use let val = call() with crash block.
/// No manual if err { ... } checks.
pub struct NoManualErrRule;

impl Rule for NoManualErrRule {
    fn name(&self) -> &'static str { "no-manual-err" }

    fn check_function(&self, ctx: &FnCheckContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        let mut err_vars: Vec<String> = Vec::new();

        // Struct methods that mutate state (contain let bindings) may use let val, err = call()
        let is_mutation_method = ctx.func.parent_struct.is_some()
            && ctx.func.def.body.iter().any(|s| matches!(s, crate::ast::Stmt::Let { .. } | crate::ast::Stmt::FieldAssign { .. }));

        for stmt in &ctx.func.def.body {
            if !is_mutation_method {
                check_let_result(stmt, &ctx.func.qualified_name, &mut errors);
            }
            collect_err_vars(stmt, &mut err_vars);
        }

        for stmt in &ctx.func.def.body {
            check_manual_err_use(stmt, &err_vars, &ctx.func.qualified_name, &mut errors);
        }

        errors
    }
}

fn is_safe_cast(expr: &Expr) -> bool {
    if let Expr::Call { target, .. } = expr {
        if let Expr::Ident(name) = target.as_ref() {
            return matches!(name.as_str(), "String" | "Number" | "Bool");
        }
    }
    false
}

fn check_let_result(stmt: &Stmt, ctx: &str, errors: &mut Vec<RuleError>) {
    match stmt {
        Stmt::LetResult { value, .. } if !is_safe_cast(value) => {
            errors.push(RuleError::new(errors::ERR_IN_BODY, "use crash block to handle errors — not let val, err = call()", Some(ctx.to_string())));
        }
        Stmt::If { then_body, else_body, .. } => {
            for s in then_body { check_let_result(s, ctx, errors); }
            if let Some(body) = else_body { for s in body { check_let_result(s, ctx, errors); } }
        }
        Stmt::For { body, .. } | Stmt::While { body, .. } => {
            for s in body { check_let_result(s, ctx, errors); }
        }
        _ => {}
    }
}

fn collect_err_vars(stmt: &Stmt, vars: &mut Vec<String>) {
    match stmt {
        Stmt::LetResult { err_name, .. } => {
            vars.push(err_name.clone());
        }
        Stmt::Wait { failed_name, .. } => {
            vars.push(failed_name.clone());
        }
        Stmt::If { then_body, else_body, .. } => {
            for s in then_body { collect_err_vars(s, vars); }
            if let Some(body) = else_body { for s in body { collect_err_vars(s, vars); } }
        }
        Stmt::For { body, .. } | Stmt::While { body, .. } => {
            for s in body { collect_err_vars(s, vars); }
        }
        _ => {}
    }
}

fn check_manual_err_use(stmt: &Stmt, err_vars: &[String], ctx: &str, errors: &mut Vec<RuleError>) {
    match stmt {
        Stmt::If { condition, then_body, else_body, .. } => {
            // Check if the condition directly references an err variable
            if let Some(var) = expr_is_err_check(condition, err_vars) {
                errors.push(RuleError::new(errors::MANUAL_ERR_CHECK, format!("'{}' should be handled in the crash block, not with if {}", var, var), Some(ctx.to_string())));
            }
            for s in then_body { check_manual_err_use(s, err_vars, ctx, errors); }
            if let Some(body) = else_body { for s in body { check_manual_err_use(s, err_vars, ctx, errors); } }
        }
        Stmt::For { body, .. } | Stmt::While { body, .. } => {
            for s in body { check_manual_err_use(s, err_vars, ctx, errors); }
        }
        _ => {}
    }
}

fn expr_is_err_check(expr: &Expr, err_vars: &[String]) -> Option<String> {
    match expr {
        // if err { ... }
        Expr::Ident(name) if err_vars.contains(name) => Some(name.clone()),
        // if err != null { ... }
        Expr::BinOp { left, right, .. } => {
            if let Expr::Ident(name) = left.as_ref() {
                if err_vars.contains(name) { return Some(name.clone()); }
            }
            if let Expr::Ident(name) = right.as_ref() {
                if err_vars.contains(name) { return Some(name.clone()); }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::check;

    fn errors(src: &str) -> Vec<crate::errors::RuleError> {
        check::check(&crate::parse::parse(src))
    }

    #[test]
    fn let_result_banned() {
        let e = errors(r#"
            pub fn bad() -> String {
                let result, err = validate("x")
                return result
                crash { validate -> skip }
                test { self() == "ok" }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "err-in-body"),
            "expected err-in-body, got: {:?}", e);
    }

    #[test]
    fn safe_cast_allowed() {
        let e = errors(r#"
            pub fn ok() -> Number {
                let n, err = Number("42")
                if err { return 0 }
                return n
                test { self() == 42 }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "err-in-body"),
            "safe casts should be allowed, got: {:?}", e);
    }

    #[test]
    fn crash_fallback_clean() {
        let e = errors(r#"
            pub fn good() -> String {
                const result = validate("x")
                return result
                crash { validate -> fallback("default") }
                test { self() == "ok" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "err-in-body"));
        assert!(!e.iter().any(|e| e.code == "manual-err-check"));
    }

    #[test]
    fn manual_err_check_banned() {
        let e = errors(r#"
            /// Does something
            pub fn bad() -> String {
                let n, err = Number("42")
                if err { return "0" }
                return "ok"
                test { self() == "ok" }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "manual-err-check"),
            "expected manual-err-check for if err, got: {:?}", e);
    }

    #[test]
    fn no_manual_err_check_without_err_var() {
        let e = errors(r#"
            /// Checks a condition
            pub fn check(x: Number) -> String {
                if x > 0 { return "positive" }
                return "zero"
                test { self(1) == "positive" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "manual-err-check"),
            "normal if should not trigger manual-err-check, got: {:?}", e);
    }
}
