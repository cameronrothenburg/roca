//! Rule: missing-crash, crash-on-safe, panic-warning
//! Validates crash block presence and strategy correctness.

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
    fn stdlib_calls_no_crash_needed() {
        // stdlib methods don't return errors — no crash block required
        let e = errors(r#"fn p(n: String) -> String { let t = n.trim() return t test { self("a") == "a" } }"#);
        assert!(!e.iter().any(|e| e.code == "missing-crash"),
            "stdlib calls should not require crash, got: {:?}", e);
    }

    #[test]
    fn no_calls_no_crash() {
        assert!(!errors(r#"fn add(a: Number, b: Number) -> Number { return a + b test { self(1, 2) == 3 } }"#)
            .iter().any(|e| e.code == "missing-crash"));
    }

    #[test]
    fn err_function_needs_crash() {
        let e = errors(r#"
            pub fn validate(s: String) -> String, err {
                err empty = "empty"
                if s == "" { return err.empty }
                return s
                test { self("a") == "a" self("") is err.empty }
            }
            pub fn caller() -> String, err {
                err empty = "empty"
                const r = validate("x")
                return r
                test { self() == "x" }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "missing-crash"),
            "expected missing-crash for err-returning function, got: {:?}", e);
    }

    #[test]
    fn err_function_with_crash_passes() {
        let e = errors(r#"
            pub fn validate(s: String) -> String, err {
                err empty = "empty"
                if s == "" { return err.empty }
                return s
                test { self("a") == "a" self("") is err.empty }
            }
            pub fn caller() -> String, err {
                err empty = "empty"
                const r = validate("x")
                return r
                crash { validate -> halt }
                test { self() == "x" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "missing-crash"),
            "crash on err function should pass, got: {:?}", e);
    }

    #[test]
    fn unhandled_err_call() {
        let e = errors(r#"
            pub fn a() -> String, err {
                err fail = "fail"
                return "ok"
                test { self() == "ok" }
            }
            pub fn b() -> String, err {
                err fail = "fail"
                return "ok"
                test { self() == "ok" }
            }
            pub fn caller() -> String, err {
                err fail = "fail"
                const x = a()
                const y = b()
                return x
                crash { a -> halt }
                test { self() == "ok" }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "unhandled-call"),
            "expected unhandled-call for b(), got: {:?}", e);
    }

    #[test]
    fn extern_fn_call_needs_crash() {
        let e = errors(r#"
            extern fn fetch(url: String) -> String, err {
                err net = "net"
            }
            fn go() -> String, err {
                let r, e = wait fetch("x")
                if e != null { return err.net }
                return r
                test { self("x") == "ok" }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "missing-crash"), "expected missing-crash, got: {:?}", e);
    }

    #[test]
    fn extern_fn_call_with_crash_passes() {
        let e = errors(r#"
            extern fn fetch(url: String) -> String, err {
                err net = "net"
            }
            fn go() -> String, err {
                let r, e = wait fetch("x")
                if e != null { return err.net }
                return r
                crash { fetch -> halt }
                test { self("x") == "ok" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "missing-crash"), "unexpected missing-crash: {:?}", e);
        assert!(!e.iter().any(|e| e.code == "unhandled-call"), "unexpected unhandled-call: {:?}", e);
    }

    #[test]
    fn closure_calls_not_collected() {
        let e = errors(r#"fn p(items: Array<String>) -> Array<String> { let r = items.map(fn(x) -> x.trim()) return r test { self(["a"]) == ["a"] } }"#);
        assert!(!e.iter().any(|e| e.code == "missing-crash"),
            "closure + stdlib calls should not require crash, got: {:?}", e);
    }

    #[test]
    fn panic_warning_fires() {
        let e = errors(r#"
            /// Does something risky
            pub fn risky(s: String) -> String, err {
                err fail = "fail"
                return s
                crash { risky -> panic }
                test { self("a") == "a" }
            }
            /// Calls risky
            pub fn caller() -> String {
                const r = risky("x")
                return r
                crash { risky -> panic }
                test { self() == "x" }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "panic-warning"),
            "expected panic-warning, got: {:?}", e);
    }

    #[test]
    fn chain_ending_with_log_rejected() {
        let e = errors(r#"
            /// Risky
            pub fn risky(s: String) -> String, err {
                err fail = "fail"
                return s
                test { self("a") == "a" self("") is err.fail }
            }
            /// Calls risky
            pub fn caller() -> String {
                const r = risky("x")
                return r
                crash { risky -> log }
                test { self() == "x" }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "nonterminal-chain"),
            "expected nonterminal-chain for log-only chain, got: {:?}", e);
    }

    #[test]
    fn chain_log_then_halt_ok() {
        let e = errors(r#"
            /// Risky
            pub fn risky(s: String) -> String, err {
                err fail = "fail"
                return s
                test { self("a") == "a" self("") is err.fail }
            }
            /// Calls risky
            pub fn caller() -> String, err {
                err fail = "fail"
                const r = risky("x")
                return r
                crash { risky -> log |> halt }
                test { self() == "x" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "nonterminal-chain"),
            "log |> halt should be fine, got: {:?}", e);
    }

    #[test]
    fn halt_no_panic_warning() {
        let e = errors(r#"
            /// Does something risky
            pub fn risky(s: String) -> String, err {
                err fail = "fail"
                return s
                test { self("a") == "a" }
            }
            /// Calls risky
            pub fn caller() -> String, err {
                err fail = "fail"
                const r = risky("x")
                return r
                crash { risky -> halt }
                test { self() == "x" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "panic-warning"),
            "halt should not trigger panic-warning, got: {:?}", e);
    }

    #[test]
    fn crash_on_safe_positive() {
        let e = errors(r#"
            /// Validates input
            pub fn validate(s: String) -> String, err {
                err empty = "empty"
                if s == "" { return err.empty }
                return s
                test { self("a") == "a" self("") is err.empty }
            }
            /// Uses validate
            pub fn caller() -> String, err {
                err empty = "empty"
                const r = validate("x")
                return r
                crash { validate -> halt }
                test { self() == "x" }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "crash-on-safe"),
            "halt on error-returning function should be fine, got: {:?}", e);
    }
}

pub struct CrashRule;

impl Rule for CrashRule {
    fn name(&self) -> &'static str { "crash" }

    fn check_function(&self, ctx: &FnCheckContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        let f = ctx.func.def;
        let calls = collect_calls(&f.body);

        let scope = build_scope(f, ctx.check.registry);

        // Only calls to error-returning functions need crash entries
        let err_calls: Vec<&String> = calls.iter()
            .filter(|c| ctx.check.registry.call_returns_err_with_scope(c, ctx.check.file, &scope, ctx.check.source_dir.as_deref()) == Some(true))
            .collect();

        if !err_calls.is_empty() {
            let crash = match &f.crash {
                Some(c) => c,
                None => {
                    for call in &err_calls {
                        errors.push(RuleError::new(errors::MISSING_CRASH, format!("'{}' returns errors but has no crash handler in '{}'", call, f.name), Some(ctx.func.qualified_name.clone())));
                    }
                    return errors;
                }
            };

            let handled: Vec<&str> = crash.handlers.iter().map(|h| h.call.as_str()).collect();
            for call in &err_calls {
                if !handled.iter().any(|h| *h == call.as_str()) {
                    errors.push(RuleError::new(errors::UNHANDLED_CALL, format!("'{}' returns errors but has no crash handler in '{}'", call, f.name), Some(ctx.func.qualified_name.clone())));
                }
            }
        }

        // Check each crash handler for misuse: crash-on-safe and panic warnings
        let crash = match &f.crash {
            Some(c) => c,
            None => return errors,
        };
        for handler in &crash.handlers {
            let returns_err = ctx.check.registry.call_returns_err_with_scope(&handler.call, ctx.check.file, &scope, ctx.check.source_dir.as_deref());

            // Crash entries with halt/fallback/retry on non-error functions are invalid —
            // these strategies destructure {value, err} but non-error functions return plain values
            if returns_err == Some(false) {
                let has_unwrap = match &handler.strategy {
                    CrashHandlerKind::Simple(chain) => chain.iter().any(|s| matches!(s, CrashStep::Halt | CrashStep::Fallback(_) | CrashStep::Retry { .. })),
                    CrashHandlerKind::Detailed { .. } => true,
                };
                if has_unwrap {
                    errors.push(RuleError::new(errors::CRASH_ON_SAFE, format!("'{}' does not return errors — remove from crash block", handler.call), Some(ctx.func.qualified_name.clone())));
                }
            }

            // Check chain validity: nonterminal ending + panic warning in one pass
            let (ends_nonterminal, has_panic) = match &handler.strategy {
                CrashHandlerKind::Simple(chain) => (
                    chain.last().map_or(false, |s| matches!(s, CrashStep::Log | CrashStep::Retry { .. })),
                    chain.iter().any(|s| matches!(s, CrashStep::Panic)),
                ),
                CrashHandlerKind::Detailed { arms, default } => {
                    let nt = arms.iter().any(|a| a.chain.last().map_or(false, |s| matches!(s, CrashStep::Log | CrashStep::Retry { .. })))
                        || default.as_ref().map_or(false, |c| c.last().map_or(false, |s| matches!(s, CrashStep::Log | CrashStep::Retry { .. })));
                    let p = arms.iter().any(|a| a.chain.iter().any(|s| matches!(s, CrashStep::Panic)))
                        || default.as_ref().map_or(false, |c| c.iter().any(|s| matches!(s, CrashStep::Panic)));
                    (nt, p)
                }
            };
            if ends_nonterminal {
                errors.push(RuleError::new(errors::NONTERMINAL_CHAIN, format!("crash chain for '{}' ends with a non-terminal strategy — must end with halt, fallback, skip, or panic", handler.call), Some(ctx.func.qualified_name.clone())));
            }
            if has_panic {
                errors.push(RuleError::new(errors::PANIC_WARNING, format!("'{}' uses panic — this will crash the process. Use halt or fallback unless this is truly unrecoverable", handler.call), Some(ctx.func.qualified_name.clone())));
            }
        }

        errors
    }
}

/// Build a scope mapping variable names to type names from params and local declarations.
/// e.g. param `intl: DateFormatting` → scope["intl"] = "DateFormatting"
/// e.g. `const formatter = intl.dateTime(...)` → scope["formatter"] = return type of dateTime
fn build_scope(f: &FnDef, registry: &crate::check::registry::ContractRegistry) -> std::collections::HashMap<String, String> {
    let mut scope = std::collections::HashMap::new();

    for p in &f.params {
        if let TypeRef::Named(t) = &p.type_ref {
            scope.insert(p.name.clone(), t.clone());
        }
    }

    // Local const/let declarations — infer type from call return types
    collect_scope_from_stmts(&f.body, &mut scope, registry);

    scope
}

/// Walk statements (including nested blocks) to find variable declarations with contract types.
fn collect_scope_from_stmts(
    stmts: &[Stmt],
    scope: &mut std::collections::HashMap<String, String>,
    registry: &crate::check::registry::ContractRegistry,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Const { name, value, .. } | Stmt::Let { name, value, .. } => {
                if let Some(type_name) = infer_call_return_type(value, scope, registry) {
                    scope.insert(name.clone(), type_name);
                }
            }
            Stmt::If { then_body, else_body, .. } => {
                collect_scope_from_stmts(then_body, scope, registry);
                if let Some(body) = else_body { collect_scope_from_stmts(body, scope, registry); }
            }
            Stmt::For { body, .. } | Stmt::While { body, .. } => {
                collect_scope_from_stmts(body, scope, registry);
            }
            _ => {}
        }
    }
}

/// Infer the return type of a call expression using the scope and registry.
fn infer_call_return_type(
    expr: &Expr,
    scope: &std::collections::HashMap<String, String>,
    registry: &crate::check::registry::ContractRegistry,
) -> Option<String> {
    match expr {
        Expr::Call { target, .. } => {
            if let Expr::FieldAccess { target: obj, field } = target.as_ref() {
                // obj.method() — resolve obj type, find method return type
                let obj_type = match obj.as_ref() {
                    Expr::Ident(name) => scope.get(name).cloned()
                        .or_else(|| registry.get(name).map(|_| name.clone())),
                    _ => None,
                };
                if let Some(type_name) = obj_type {
                    if let Some(contract) = registry.get(&type_name) {
                        if let Some(sig) = contract.functions.iter().find(|f| f.name == *field) {
                            return match &sig.return_type {
                                TypeRef::Named(n) => Some(n.clone()),
                                _ => None,
                            };
                        }
                    }
                }
            }
            None
        }
        Expr::Await(inner) => infer_call_return_type(inner, scope, registry),
        _ => None,
    }
}

fn collect_calls(stmts: &[Stmt]) -> Vec<String> {
    let mut calls = Vec::new();
    for stmt in stmts { collect_calls_in_stmt(stmt, &mut calls); }
    calls
}

fn collect_calls_in_stmt(stmt: &Stmt, calls: &mut Vec<String>) {
    match stmt {
        Stmt::Const { value, .. } | Stmt::Let { value, .. } | Stmt::Assign { value, .. }
        | Stmt::FieldAssign { value, .. } | Stmt::Return(value) | Stmt::Expr(value) => {
            collect_calls_in_expr(value, calls);
        }
        Stmt::LetResult { value, .. } => collect_calls_in_expr(value, calls),
        Stmt::ReturnErr { .. } | Stmt::Break | Stmt::Continue => {}
        Stmt::If { condition, then_body, else_body } => {
            collect_calls_in_expr(condition, calls);
            for s in then_body { collect_calls_in_stmt(s, calls); }
            if let Some(body) = else_body { for s in body { collect_calls_in_stmt(s, calls); } }
        }
        Stmt::For { iter, body, .. } => {
            collect_calls_in_expr(iter, calls);
            for s in body { collect_calls_in_stmt(s, calls); }
        }
        Stmt::While { condition, body } => {
            collect_calls_in_expr(condition, calls);
            for s in body { collect_calls_in_stmt(s, calls); }
        }
        Stmt::Wait { kind, .. } => match kind {
            WaitKind::Single(e) => collect_calls_in_expr(e, calls),
            WaitKind::All(es) | WaitKind::First(es) => { for e in es { collect_calls_in_expr(e, calls); } }
        },
    }
}

fn collect_calls_in_expr(expr: &Expr, calls: &mut Vec<String>) {
    match expr {
        Expr::Call { target, args } => {
            if let Some(name) = crate::ast::expr_to_dotted_name(target) {
                if !calls.contains(&name) { calls.push(name); }
            }
            for a in args { collect_calls_in_expr(a, calls); }
        }
        Expr::BinOp { left, right, .. } => {
            collect_calls_in_expr(left, calls);
            collect_calls_in_expr(right, calls);
        }
        Expr::Await(inner) => collect_calls_in_expr(inner, calls),
        Expr::FieldAccess { target, .. } => collect_calls_in_expr(target, calls),
        Expr::StructLit { fields, .. } => { for (_, v) in fields { collect_calls_in_expr(v, calls); } }
        Expr::Array(elements) => { for e in elements { collect_calls_in_expr(e, calls); } }
        Expr::Index { target, index } => {
            collect_calls_in_expr(target, calls);
            collect_calls_in_expr(index, calls);
        }
        Expr::Match { value, arms } => {
            collect_calls_in_expr(value, calls);
            for arm in arms {
                if let Some(crate::ast::MatchPattern::Value(p)) = &arm.pattern { collect_calls_in_expr(p, calls); }
                collect_calls_in_expr(&arm.value, calls);
            }
        }
        _ => {}
    }
}

