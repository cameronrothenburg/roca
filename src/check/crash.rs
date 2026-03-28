use crate::ast::*;
use crate::errors::RuleError;

/// Validate every function call has a crash handler
pub fn check_crash(file: &SourceFile) -> Vec<RuleError> {
    let mut errors = Vec::new();

    for item in &file.items {
        match item {
            Item::Function(f) => check_fn_crash(f, None, &mut errors),
            Item::Struct(s) => {
                for m in &s.methods {
                    check_fn_crash(m, Some(&s.name), &mut errors);
                }
            }
            Item::Satisfies(sat) => {
                for m in &sat.methods {
                    check_fn_crash(m, Some(&sat.struct_name), &mut errors);
                }
            }
            _ => {}
        }
    }

    errors
}

fn check_fn_crash(f: &FnDef, parent: Option<&str>, errors: &mut Vec<RuleError>) {
    // Collect all function calls in the body
    let calls = collect_calls(&f.body);

    if calls.is_empty() {
        return;
    }

    // Must have a crash block
    let crash = match &f.crash {
        Some(c) => c,
        None => {
            let context = match parent {
                Some(p) => format!("{}.{}", p, f.name),
                None => f.name.clone(),
            };
            errors.push(RuleError {
                code: "missing-crash".into(),
                message: format!(
                    "function '{}' makes calls but has no crash block",
                    f.name
                ),
                context: Some(context),
            });
            return;
        }
    };

    // Every call must have a handler in the crash block
    let handled: Vec<&str> = crash.handlers.iter().map(|h| h.call.as_str()).collect();

    for call in &calls {
        if !handled.iter().any(|h| *h == call.as_str()) {
            let context = match parent {
                Some(p) => format!("{}.{}", p, f.name),
                None => f.name.clone(),
            };
            errors.push(RuleError {
                code: "unhandled-call".into(),
                message: format!(
                    "call '{}' has no crash handler in '{}'",
                    call, f.name
                ),
                context: Some(context),
            });
        }
    }
}

/// Collect all function/method call targets from a list of statements
fn collect_calls(stmts: &[Stmt]) -> Vec<String> {
    let mut calls = Vec::new();
    for stmt in stmts {
        collect_calls_in_stmt(stmt, &mut calls);
    }
    calls
}

fn collect_calls_in_stmt(stmt: &Stmt, calls: &mut Vec<String>) {
    match stmt {
        Stmt::Const { value, .. }
        | Stmt::Let { value, .. }
        | Stmt::Assign { value, .. }
        | Stmt::Return(value)
        | Stmt::Expr(value) => {
            collect_calls_in_expr(value, calls);
        }
        Stmt::LetResult { value, .. } => {
            collect_calls_in_expr(value, calls);
        }
        Stmt::ReturnErr(_) => {}
        Stmt::If { condition, then_body, else_body } => {
            collect_calls_in_expr(condition, calls);
            for s in then_body {
                collect_calls_in_stmt(s, calls);
            }
            if let Some(body) = else_body {
                for s in body {
                    collect_calls_in_stmt(s, calls);
                }
            }
        }
        Stmt::For { iter, body, .. } => {
            collect_calls_in_expr(iter, calls);
            for s in body {
                collect_calls_in_stmt(s, calls);
            }
        }
        Stmt::Wait { kind, .. } => {
            match kind {
                crate::ast::WaitKind::Single(expr) => collect_calls_in_expr(expr, calls),
                crate::ast::WaitKind::All(exprs) | crate::ast::WaitKind::First(exprs) => {
                    for e in exprs { collect_calls_in_expr(e, calls); }
                }
            }
        }
    }
}

fn collect_calls_in_expr(expr: &Expr, calls: &mut Vec<String>) {
    match expr {
        Expr::Call { target, args } => {
            // Build the call target string
            let call_name = expr_to_call_name(target);
            if let Some(name) = call_name {
                if !calls.contains(&name) {
                    calls.push(name);
                }
            }
            for arg in args {
                collect_calls_in_expr(arg, calls);
            }
        }
        Expr::BinOp { left, right, .. } => {
            collect_calls_in_expr(left, calls);
            collect_calls_in_expr(right, calls);
        }
        Expr::FieldAccess { target, .. } => {
            collect_calls_in_expr(target, calls);
        }
        Expr::StructLit { fields, .. } => {
            for (_, val) in fields {
                collect_calls_in_expr(val, calls);
            }
        }
        Expr::Array(elements) => {
            for el in elements {
                collect_calls_in_expr(el, calls);
            }
        }
        Expr::Index { target, index } => {
            collect_calls_in_expr(target, calls);
            collect_calls_in_expr(index, calls);
        }
        Expr::Match { value, arms } => {
            collect_calls_in_expr(value, calls);
            for arm in arms {
                if let Some(p) = &arm.pattern { collect_calls_in_expr(p, calls); }
                collect_calls_in_expr(&arm.value, calls);
            }
        }
        _ => {}
    }
}

/// Convert an expression to a dotted call name (e.g. "http.get", "name.trim")
fn expr_to_call_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::FieldAccess { target, field } => {
            let parent = match target.as_ref() {
                Expr::Ident(name) => Some(name.clone()),
                Expr::SelfRef => Some("self".to_string()),
                Expr::FieldAccess { .. } => expr_to_call_name(target),
                _ => None,
            };
            parent.map(|p| format!("{}.{}", p, field))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn function_with_crash_passes() {
        let file = parse::parse(r#"
            fn process(name: String) -> String {
                let trimmed = name.trim()
                return trimmed
                crash { name.trim -> halt }
                test { self("cam") == "cam" }
            }
        "#);
        let errors = check_crash(&file);
        assert!(errors.is_empty());
    }

    #[test]
    fn missing_crash_block() {
        let file = parse::parse(r#"
            fn process(name: String) -> String {
                let trimmed = name.trim()
                return trimmed
                test { self("cam") == "cam" }
            }
        "#);
        let errors = check_crash(&file);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "missing-crash");
    }

    #[test]
    fn unhandled_call() {
        let file = parse::parse(r#"
            fn process(name: String) -> String {
                let trimmed = name.trim()
                let upper = trimmed.to_upper()
                return upper
                crash { name.trim -> halt }
                test { self("cam") == "CAM" }
            }
        "#);
        let errors = check_crash(&file);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "unhandled-call");
    }

    #[test]
    fn no_calls_no_crash_needed() {
        let file = parse::parse(r#"
            fn add(a: Number, b: Number) -> Number {
                return a + b
                test { self(1, 2) == 3 }
            }
        "#);
        let errors = check_crash(&file);
        assert!(errors.is_empty());
    }
}
