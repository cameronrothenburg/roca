//! AST walker — traverses functions and statements, builds scopes, and invokes rules.

use roca_ast::*;
use roca_errors::RuleError;
use super::context::*;
use super::rule::Rule;
use roca_resolve::ContractRegistry;

// ─── Public type utilities (extracted from methods.rs) ──

pub fn type_ref_to_name(t: &TypeRef) -> String {
    match t {
        TypeRef::String => "String".to_string(),
        TypeRef::Number => "Number".to_string(),
        TypeRef::Bool => "Bool".to_string(),
        TypeRef::Named(n) => n.clone(),
        TypeRef::Generic(name, args) => {
            let arg_names: Vec<String> = args.iter().map(|a| type_ref_to_name(a)).collect();
            format!("{}<{}>", name, arg_names.join(", "))
        }
        TypeRef::Nullable(inner) => format!("{}?", type_ref_to_name(inner)),
        TypeRef::Fn(params, ret) => {
            let p: Vec<String> = params.iter().map(|t| type_ref_to_name(t)).collect();
            format!("fn({}) -> {}", p.join(", "), type_ref_to_name(ret))
        }
        TypeRef::Ok => "Ok".to_string(),
    }
}

/// Get just the type name from scope (convenience for existing code)
pub fn scope_type(scope: &Scope, name: &str) -> Option<String> {
    scope.get(name).map(|v| v.type_name.clone())
}

pub fn resolve_type(expr: &Expr, scope: &Scope) -> Option<String> {
    match expr {
        Expr::Ident(name) => scope_type(scope, name),
        Expr::String(_) | Expr::StringInterp(_) => Some("String".to_string()),
        Expr::Number(_) => Some("Number".to_string()),
        Expr::Bool(_) => Some("Bool".to_string()),
        Expr::Array(elements) => {
            if let Some(first) = elements.first() {
                if let Some(elem_type) = resolve_type(first, scope) {
                    return Some(format!("Array<{}>", elem_type));
                }
            }
            Some("Array".to_string())
        }
        Expr::Null => None,
        Expr::SelfRef => None,
        Expr::FieldAccess { target, field } => {
            if let Some(type_name) = resolve_type(target, scope) {
                let base = type_name.split('<').next().unwrap_or(&type_name);
                let key = format!("{}.{}",
                    if matches!(target.as_ref(), Expr::SelfRef) { "self" } else { base },
                    field
                );
                if let Some(t) = scope_type(scope, &key) {
                    return Some(t);
                }
            }
            None
        }
        _ => None,
    }
}

pub fn infer_type_with_registry(expr: &Expr, scope: &Scope, registry: Option<&ContractRegistry>) -> Option<String> {
    match expr {
        Expr::Call { target, .. } => {
            if let Expr::Ident(name) = target.as_ref() {
                if name.chars().next().map_or(false, |c| c.is_uppercase()) {
                    return Some(name.clone());
                }
            }
            if let Expr::FieldAccess { target: obj, field } = target.as_ref() {
                if let Expr::Ident(name) = obj.as_ref() {
                    if name.chars().next().map_or(false, |c| c.is_uppercase()) {
                        // Check registry for method return type (e.g. JSON.parse → String)
                        if let Some(reg) = registry {
                            if let Some(contract) = reg.get(name) {
                                if let Some(sig) = contract.functions.iter().find(|f| f.name == *field) {
                                    return Some(type_ref_to_name(&sig.return_type));
                                }
                            }
                        }
                        // Fallback: assume constructor pattern (Email.validate → Email)
                        return Some(name.clone());
                    }
                }
                if let Some(type_name) = resolve_type(obj, scope) {
                    if type_name == "String" {
                        return match field.as_str() {
                            "includes" | "startsWith" | "endsWith" => Some("Bool".to_string()),
                            "indexOf" | "charCodeAt" | "length" => Some("Number".to_string()),
                            "split" => Some("Array".to_string()),
                            _ => Some("String".to_string()),
                        };
                    }
                    // Check registry for method return type on any resolved type
                    if let Some(reg) = registry {
                        if let Some(contract) = reg.get(&type_name) {
                            if let Some(sig) = contract.functions.iter().find(|f| f.name == *field) {
                                return Some(type_ref_to_name(&sig.return_type));
                            }
                        }
                    }
                }
            }
            None
        }
        Expr::String(_) | Expr::StringInterp(_) => Some("String".to_string()),
        Expr::Number(_) => Some("Number".to_string()),
        Expr::Bool(_) => Some("Bool".to_string()),
        Expr::Array(elements) => {
            if let Some(first) = elements.first() {
                if let Some(elem_type) = resolve_type(first, scope) {
                    return Some(format!("Array<{}>", elem_type));
                }
            }
            Some("Array".to_string())
        }
        Expr::Ident(name) => scope_type(scope, name),
        _ => None,
    }
}

// ─── Walker ─────────────────────────────────────────────

pub fn walk(file: &SourceFile, registry: &ContractRegistry, source_dir: Option<&std::path::Path>, rules: &[Box<dyn Rule>]) -> Vec<RuleError> {
    let mut errors = Vec::new();
    let check = CheckContext { file, registry, source_dir: source_dir.map(|p| p.to_path_buf()) };

    for item in &file.items {
        let item_ctx = ItemContext { check: &check, item };
        for rule in rules {
            errors.extend(rule.check_item(&item_ctx));
        }

        match item {
            Item::Function(f) => {
                let fn_ctx = FnContext {
                    def: f,
                    qualified_name: f.name.clone(),
                    parent_struct: None,
                };
                walk_function(&check, &fn_ctx, rules, &mut errors);
            }
            Item::Struct(s) => {
                for method in &s.methods {
                    let fn_ctx = FnContext {
                        def: method,
                        qualified_name: format!("{}.{}", s.name, method.name),
                        parent_struct: Some(&s.name),
                    };
                    walk_function_with_fields(&check, &fn_ctx, &s.fields, rules, &mut errors);
                }
            }
            Item::Satisfies(sat) => {
                for method in &sat.methods {
                    let fn_ctx = FnContext {
                        def: method,
                        qualified_name: format!("{}.{}", sat.struct_name, method.name),
                        parent_struct: Some(&sat.struct_name),
                    };
                    walk_function(&check, &fn_ctx, rules, &mut errors);
                }
            }
            _ => {}
        }
    }

    errors
}

fn walk_function(check: &CheckContext, fn_ctx: &FnContext, rules: &[Box<dyn Rule>], errors: &mut Vec<RuleError>) {
    let scope = build_scope(&fn_ctx.def.params);
    walk_function_inner(check, fn_ctx, scope, rules, errors);
}

fn walk_function_with_fields(check: &CheckContext, fn_ctx: &FnContext, fields: &[Field], rules: &[Box<dyn Rule>], errors: &mut Vec<RuleError>) {
    let mut scope = build_scope(&fn_ctx.def.params);
    for field in fields {
        scope.insert(format!("self.{}", field.name), VarInfo::new_const(type_ref_to_name(&field.type_ref)));
    }
    walk_function_inner(check, fn_ctx, scope, rules, errors);
}

fn walk_function_inner(check: &CheckContext, fn_ctx: &FnContext, scope: Scope, rules: &[Box<dyn Rule>], errors: &mut Vec<RuleError>) {
    let fn_check = FnCheckContext { check, func: fn_ctx.clone() };
    for rule in rules {
        errors.extend(rule.check_function(&fn_check));
    }
    walk_stmts(&fn_ctx.def.body, check, fn_ctx, scope, rules, errors);
}

fn walk_stmts(stmts: &[Stmt], check: &CheckContext, fn_ctx: &FnContext, mut scope: Scope, rules: &[Box<dyn Rule>], errors: &mut Vec<RuleError>) {
    for stmt in stmts {
        let stmt_ctx = StmtContext { check, func: fn_ctx, scope: &scope, stmt };
        for rule in rules {
            errors.extend(rule.check_stmt(&stmt_ctx));
        }

        match stmt {
            Stmt::Const { name, value, type_ann, .. } => {
                walk_expr(value, check, fn_ctx, &scope, rules, errors);
                let type_name = type_ann.as_ref().map(type_ref_to_name)
                    .or_else(|| infer_type_with_registry(value, &scope, Some(check.registry)));
                if let Some(t) = type_name {
                    scope.insert(name.clone(), VarInfo::new_const(t));
                }
            }
            Stmt::Let { name, value, type_ann, .. } => {
                walk_expr(value, check, fn_ctx, &scope, rules, errors);
                let type_name = type_ann.as_ref().map(type_ref_to_name)
                    .or_else(|| infer_type_with_registry(value, &scope, Some(check.registry)));
                if let Some(t) = type_name {
                    scope.insert(name.clone(), VarInfo::new_let(t));
                }
            }
            Stmt::LetResult { value, .. } => {
                walk_expr(value, check, fn_ctx, &scope, rules, errors);
            }
            Stmt::Return(expr) | Stmt::Expr(expr) => {
                walk_expr(expr, check, fn_ctx, &scope, rules, errors);
            }
            Stmt::Assign { value, .. } | Stmt::FieldAssign { value, .. } => {
                walk_expr(value, check, fn_ctx, &scope, rules, errors);
            }
            Stmt::ReturnErr { .. } | Stmt::Break | Stmt::Continue => {}
            Stmt::If { condition, then_body, else_body } => {
                walk_expr(condition, check, fn_ctx, &scope, rules, errors);

                if let Some((var_name, is_eq_null)) = extract_null_check(condition) {
                    if is_eq_null {
                        walk_stmts(then_body, check, fn_ctx, scope.clone(), rules, errors);
                        if body_exits(then_body) {
                            if let Some(v) = scope.get(&var_name).cloned() {
                                if v.type_name.ends_with('?') {
                                    scope.insert(var_name, VarInfo { type_name: v.type_name.trim_end_matches('?').to_string(), ..v });
                                }
                            }
                        }
                    } else {
                        let mut then_scope = scope.clone();
                        if let Some(v) = then_scope.get(&var_name).cloned() {
                            if v.type_name.ends_with('?') {
                                then_scope.insert(var_name, VarInfo { type_name: v.type_name.trim_end_matches('?').to_string(), ..v });
                            }
                        }
                        walk_stmts(then_body, check, fn_ctx, then_scope, rules, errors);
                    }
                } else {
                    walk_stmts(then_body, check, fn_ctx, scope.clone(), rules, errors);
                }

                if let Some(body) = else_body {
                    walk_stmts(body, check, fn_ctx, scope.clone(), rules, errors);
                }
            }
            Stmt::For { iter, body, .. } => {
                walk_expr(iter, check, fn_ctx, &scope, rules, errors);
                walk_stmts(body, check, fn_ctx, scope.clone(), rules, errors);
            }
            Stmt::While { condition, body } => {
                walk_expr(condition, check, fn_ctx, &scope, rules, errors);
                walk_stmts(body, check, fn_ctx, scope.clone(), rules, errors);
            }
            Stmt::Wait { kind, .. } => {
                match kind {
                    WaitKind::Single(e) => walk_expr(e, check, fn_ctx, &scope, rules, errors),
                    WaitKind::All(es) | WaitKind::First(es) => {
                        for e in es { walk_expr(e, check, fn_ctx, &scope, rules, errors); }
                    }
                }
            }
        }
    }
}

fn walk_expr(expr: &Expr, check: &CheckContext, fn_ctx: &FnContext, scope: &Scope, rules: &[Box<dyn Rule>], errors: &mut Vec<RuleError>) {
    let ctx = ExprContext { check, func: fn_ctx, scope, expr };
    for rule in rules {
        errors.extend(rule.check_expr(&ctx));
    }

    match expr {
        Expr::Call { target, args } => {
            walk_expr(target, check, fn_ctx, scope, rules, errors);
            for a in args { walk_expr(a, check, fn_ctx, scope, rules, errors); }
        }
        Expr::FieldAccess { target, .. } => walk_expr(target, check, fn_ctx, scope, rules, errors),
        Expr::BinOp { left, right, .. } => {
            walk_expr(left, check, fn_ctx, scope, rules, errors);
            walk_expr(right, check, fn_ctx, scope, rules, errors);
        }
        Expr::StructLit { fields, .. } => {
            for (_, v) in fields { walk_expr(v, check, fn_ctx, scope, rules, errors); }
        }
        Expr::Array(elements) => {
            for e in elements { walk_expr(e, check, fn_ctx, scope, rules, errors); }
        }
        Expr::Index { target, index } => {
            walk_expr(target, check, fn_ctx, scope, rules, errors);
            walk_expr(index, check, fn_ctx, scope, rules, errors);
        }
        Expr::Match { value, arms } => {
            walk_expr(value, check, fn_ctx, scope, rules, errors);
            for arm in arms {
                if let Some(MatchPattern::Value(p)) = &arm.pattern { walk_expr(p, check, fn_ctx, scope, rules, errors); }
                walk_expr(&arm.value, check, fn_ctx, scope, rules, errors);
            }
        }
        Expr::Not(inner) | Expr::Await(inner) => walk_expr(inner, check, fn_ctx, scope, rules, errors),
        Expr::Closure { body, .. } => walk_expr(body, check, fn_ctx, scope, rules, errors),
        _ => {}
    }
}

fn build_scope(params: &[Param]) -> Scope {
    let mut scope = Scope::new();
    for p in params {
        scope.insert(p.name.clone(), VarInfo::new_const(type_ref_to_name(&p.type_ref)));
    }
    scope
}

/// Extracts a null check from a binary comparison.
/// Returns `(variable_name, is_eq_null)` where `is_eq_null` is true for `== null`
/// and false for `!= null`.
fn extract_null_check(expr: &Expr) -> Option<(String, bool)> {
    if let Expr::BinOp { left, op, right } = expr {
        let is_eq = match op {
            BinOp::Eq => Some(true),
            BinOp::Neq => Some(false),
            _ => None,
        }?;

        // x == null or x != null
        if let (Expr::Ident(name), Expr::Null) = (left.as_ref(), right.as_ref()) {
            return Some((name.clone(), is_eq));
        }
        // null == x or null != x
        if let (Expr::Null, Expr::Ident(name)) = (left.as_ref(), right.as_ref()) {
            return Some((name.clone(), is_eq));
        }
    }
    None
}

/// Returns true if the statement list contains an early exit (return, return err, or break).
fn body_exits(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| matches!(s, Stmt::Return(_) | Stmt::ReturnErr { .. } | Stmt::Break))
}
