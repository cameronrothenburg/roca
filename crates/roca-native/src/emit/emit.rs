//! AST walker — translates Roca AST to Body method calls.
//! Zero IR imports. Every expression is 1-3 lines, every statement is 2-5 lines.

use roca_ast::{self as roca, Expr, Stmt, BinOp, StringPart as AstStringPart};
use roca_cranelift::api::{Body, Value, StringPart, MatchArmLazy, LazyArmKind};
use roca_types::RocaType;

/// Walk a statement list, emitting each through the Body API.
pub fn emit_body(body: &mut Body, stmts: &[Stmt]) {
    for stmt in stmts {
        if body.has_returned() { break; }
        emit_stmt(body, stmt);
    }
}

fn emit_stmt(body: &mut Body, stmt: &Stmt) {
    match stmt {
        Stmt::Const { name, value, .. } | Stmt::Let { name, value, .. } => {
            if let Expr::StructLit { name: struct_name, .. } = value {
                body.set_struct_type(name, struct_name);
            }
            let kind = body.infer_type(value);
            let val = emit_expr(body, value);
            body.const_var_typed(name, val, kind);
        }
        Stmt::Return(expr) => {
            let val = emit_expr(body, expr);
            body.return_val(val);
        }
        Stmt::Expr(expr) => { emit_expr(body, expr); }
        Stmt::If { condition, then_body, else_body, .. } => {
            let cond = emit_expr(body, condition);
            let then_stmts = then_body.clone();
            let else_stmts = else_body.clone();
            body.if_else(cond,
                |b| emit_body(b, &then_stmts),
                |b| if let Some(stmts) = &else_stmts { emit_body(b, stmts); },
            );
        }
        Stmt::While { condition, body: while_body, .. } => {
            let cond_expr = condition.clone();
            let loop_body = while_body.clone();
            body.while_loop(
                |b| emit_expr(b, &cond_expr),
                |b| emit_body(b, &loop_body),
            );
        }
        Stmt::For { binding, iter, body: for_body } => {
            let arr = emit_expr(body, iter);
            let binding = binding.clone();
            let loop_body = for_body.clone();
            body.for_each(&binding, arr, |b| emit_body(b, &loop_body));
        }
        Stmt::Break => body.break_loop(),
        Stmt::Continue => body.continue_loop(),
        Stmt::Assign { name, value } => {
            let val = emit_expr(body, value);
            body.assign_name(name, val);
        }
        Stmt::FieldAssign { target, field, value } => {
            let var_name = match target {
                Expr::Ident(name) => name.as_str(),
                Expr::SelfRef => "self",
                _ => return,
            };
            let val = emit_expr(body, value);
            body.field_assign(var_name, field, val);
        }
        Stmt::LetResult { name, err_name, value } => {
            if let Expr::Call { target, args } = value {
                if let Expr::Ident(fn_name) = target.as_ref() {
                    let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();
                    body.let_result(name, err_name, fn_name, &arg_vals);
                }
            }
        }
        Stmt::ReturnErr { name, .. } => {
            body.return_err(name);
        }
        Stmt::Wait { names, failed_name, kind } => {
            emit_wait(body, names, failed_name, kind);
        }
    }
}

pub fn emit_expr(body: &mut Body, expr: &Expr) -> Value {
    match expr {
        Expr::Number(n) => body.number(*n),
        Expr::Bool(v) => body.bool_val(*v),
        Expr::String(s) => body.string(s),
        Expr::Null => body.null(),
        Expr::SelfRef => body.self_ref(),
        Expr::Ident(name) => body.var(name),
        Expr::BinOp { left, op, right } => {
            let l_is_temp_string = matches!(op, BinOp::Add)
                && !matches!(left.as_ref(), Expr::Ident(_) | Expr::Number(_) | Expr::Bool(_) | Expr::Null)
                && body.infer_type(left) == RocaType::String;
            let l = emit_expr(body, left);
            let r = emit_expr(body, right);
            let result = body.binop(op, l, r);
            if l_is_temp_string { body.release_rc(l); }
            result
        }
        Expr::Not(inner) => {
            let val = emit_expr(body, inner);
            body.not(val)
        }
        Expr::StructLit { name, fields } => emit_struct_lit(body, name, fields),
        Expr::Call { target, args } => emit_call(body, target, args),
        Expr::Array(elements) => {
            let vals: Vec<Value> = elements.iter().map(|e| emit_expr(body, e)).collect();
            body.array(&vals)
        }
        Expr::Index { target, index } => {
            let arr = emit_expr(body, target);
            let idx = emit_expr(body, index);
            body.index(arr, idx)
        }
        Expr::Closure { params, body: closure_body } => {
            let name = format!("__closure_{}_{}", params.len(), closure_hash(params, closure_body));
            body.closure_ref(&name)
        }
        Expr::StringInterp(parts) => {
            let converted: Vec<StringPart> = parts.iter().map(|p| match p {
                AstStringPart::Literal(s) => StringPart::Lit(s.clone()),
                AstStringPart::Expr(e) => StringPart::Expr(emit_expr(body, e)),
            }).collect();
            body.string_interp(&converted)
        }
        Expr::Match { value, arms } => emit_match(body, value, arms),
        Expr::FieldAccess { target, field } => emit_field_access(body, target, field),
        Expr::EnumVariant { enum_name: _, variant, args } => {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();
            body.enum_variant("", variant, &arg_vals)
        }
        Expr::Await(inner) => emit_expr(body, inner),
    }
}

fn emit_call(body: &mut Body, target: &Expr, args: &[Expr]) -> Value {
    // Method call: obj.method(args)
    if let Expr::FieldAccess { target: obj, field: method } = target {
        return emit_method_call(body, obj, method, args);
    }

    if let Expr::Ident(name) = target {
        // log() builtin
        if name == "log" {
            if let Some(arg) = args.first() {
                let val = emit_expr(body, arg);
                body.log(val);
            }
            return body.bool_val(false);
        }

        let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();

        // Known function call
        if body.has_func(name) {
            return body.call(name, &arg_vals);
        }

        // Closure call (variable holding a function pointer)
        return body.call_closure(name, &arg_vals);
    }
    body.null()
}

fn emit_method_call(body: &mut Body, target: &Expr, method: &str, args: &[Expr]) -> Value {
    // Static/type-level dispatch: Type.method(args)
    // Includes enum variant constructors, extern contract methods, struct static methods
    if let Expr::Ident(type_name) = target {
        let qualified = format!("{}.{}", type_name, method);
        // Check if this is a known function (extern contract, struct method, etc.)
        let has_func = body.has_func(&qualified);
        if has_func {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();
            return body.call(&qualified, &arg_vals);
        }
        // Check if this is an enum variant constructor
        if body.is_enum_variant(type_name, method) {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();
            return body.enum_variant(type_name, method, &arg_vals);
        }
        // Otherwise fall through to instance method dispatch
    }

    // Inline map/filter
    if (method == "map" || method == "filter") && !args.is_empty() {
        if let Expr::Closure { params, body: closure_body } = &args[0] {
            let arr = emit_expr(body, target);
            let param_name = params.first().cloned().unwrap_or_default();
            let closure_body = *closure_body.clone();
            return if method == "map" {
                body.inline_map(arr, &param_name, |b| emit_expr(b, &closure_body))
            } else {
                body.inline_filter(arr, &param_name, |b| emit_expr(b, &closure_body))
            };
        }
    }

    // Track temp strings for chained method calls
    let target_is_temp_string = !matches!(target, Expr::Ident(_) | Expr::String(_))
        && body.infer_type(target) == RocaType::String;

    let obj = emit_expr(body, target);
    let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();
    let result = body.method_call(obj, method, &arg_vals);

    if target_is_temp_string { body.release_rc(obj); }
    result
}

fn emit_field_access(body: &mut Body, target: &Expr, field: &str) -> Value {
    // Enum unit variant: Token.Plus
    if let Expr::Ident(name) = target {
        if body.is_enum_variant(name, field) {
            return body.enum_variant(name, field, &[]);
        }
    }

    let var_name = match target {
        Expr::Ident(name) => Some(name.as_str()),
        Expr::SelfRef => Some("self"),
        _ => None,
    };

    if let Some(var_name) = var_name {
        let obj = emit_expr(body, target);
        return body.field_access_on(var_name, obj, field);
    }

    let obj = emit_expr(body, target);
    body.method_call(obj, field, &[])
}

fn emit_struct_lit(body: &mut Body, name: &str, fields: &[(String, Expr)]) -> Value {
    let field_vals: Vec<(&str, Value)> = fields.iter()
        .map(|(n, e)| (n.as_str(), emit_expr(body, e)))
        .collect();

    let defs = body.struct_defs(name);
    body.struct_lit_checked(name, &field_vals, defs.as_deref())
}

fn emit_match(body: &mut Body, value: &Expr, arms: &[roca::MatchArm]) -> Value {
    let scrutinee = emit_expr(body, value);

    let default_arm = arms.iter().find(|a| a.pattern.is_none());
    let remaining: Vec<_> = arms.iter().filter(|a| a.pattern.is_some()).collect();

    // Pre-evaluate pattern values (but NOT results - they may depend on bindings)
    let mut match_arms = Vec::new();
    for arm in &remaining {
        match &arm.pattern {
            Some(roca::MatchPattern::Value(pattern)) => {
                let pat = emit_expr(body, pattern);
                match_arms.push(CompiledArm::Value { pattern: pat, value_expr: arm.value.clone() });
            }
            Some(roca::MatchPattern::Variant { variant, bindings, .. }) => {
                match_arms.push(CompiledArm::Variant {
                    variant: variant.clone(),
                    bindings: bindings.clone(),
                    value_expr: arm.value.clone(),
                });
            }
            None => {}
        }
    }

    let default_expr = default_arm.map(|a| a.value.clone());

    body.match_lazy(scrutinee, &match_arms, &default_expr, emit_expr)
}

/// Intermediate representation for match arms with deferred result evaluation.
pub enum CompiledArm {
    Value { pattern: Value, value_expr: roca::Expr },
    Variant { variant: String, bindings: Vec<String>, value_expr: roca::Expr },
}

impl MatchArmLazy for CompiledArm {
    fn kind(&self) -> LazyArmKind<'_> {
        match self {
            CompiledArm::Value { pattern, value_expr } => {
                LazyArmKind::Value { pattern, value_expr }
            }
            CompiledArm::Variant { variant, bindings, value_expr } => {
                LazyArmKind::Variant { variant, bindings, value_expr }
            }
        }
    }
}

fn emit_wait(body: &mut Body, names: &[String], failed_name: &str, kind: &roca::WaitKind) {
    match kind {
        roca::WaitKind::Single(expr) => {
            let kind = body.infer_type(expr);
            let val = emit_expr(body, expr);
            if !names.is_empty() {
                body.wait_single_typed(&names[0], val, kind);
            }
            body.bind_failed(failed_name);
        }
        roca::WaitKind::All(exprs) => {
            let fn_names: Vec<String> = exprs.iter()
                .map(|e| format!("__wait_{}", super::compile::wait_expr_hash(e)))
                .collect();
            body.wait_all(names, failed_name, &fn_names);
        }
        roca::WaitKind::First(exprs) => {
            let fn_names: Vec<String> = exprs.iter()
                .map(|e| format!("__wait_{}", super::compile::wait_expr_hash(e)))
                .collect();
            body.wait_first(names, failed_name, &fn_names);
        }
    }
}

/// Simple hash for identifying closures by their AST structure.
pub fn closure_hash(params: &[String], body: &Expr) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    for p in params { p.hash(&mut h); }
    h.finish() ^ super::compile::expr_debug_hash(body)
}
