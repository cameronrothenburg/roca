//! AST walker — translates Roca AST to Body method calls.
//! Zero IR imports. Every expression is 1-3 lines, every statement is 2-5 lines.
//!
//! Roca language semantics (stdlib dispatch, log, inline map/filter, constraint
//! validation, crash handling, type inference) live here as free functions that
//! compose Body's generic API with NativeCtx's Roca-specific metadata.

use roca_ast::{self as roca, Expr, Stmt, BinOp, StringPart as AstStringPart};
use roca_ast::crash::{CrashHandlerKind, CrashStep};
use roca_cranelift::api::{Body, Value, StringPart, MatchArmLazy, LazyArmKind};
use roca_types::RocaType;
use super::context::NativeCtx;

/// Walk a statement list, emitting each through the Body API.
pub fn emit_body(body: &mut Body, nctx: &NativeCtx, stmts: &[Stmt]) {
    for stmt in stmts {
        if body.has_returned() { break; }
        emit_stmt(body, nctx, stmt);
    }
}

fn emit_stmt(body: &mut Body, nctx: &NativeCtx, stmt: &Stmt) {
    match stmt {
        Stmt::Const { name, value, .. } | Stmt::Let { name, value, .. } => {
            if let Expr::StructLit { name: struct_name, .. } = value {
                body.set_struct_type(name, struct_name);
            }
            let kind = nctx.infer_type(value, body);
            let val = emit_expr(body, nctx, value);
            body.const_var_typed(name, val, kind);
        }
        Stmt::Return(expr) => {
            let val = emit_expr(body, nctx, expr);
            body.return_val(val);
        }
        Stmt::Expr(expr) => { emit_expr(body, nctx, expr); }
        Stmt::If { condition, then_body, else_body, .. } => {
            let cond = emit_expr(body, nctx, condition);
            let then_stmts = then_body.clone();
            let else_stmts = else_body.clone();
            body.if_else(cond,
                |b| emit_body(b, nctx, &then_stmts),
                |b| if let Some(stmts) = &else_stmts { emit_body(b, nctx, stmts); },
            );
        }
        Stmt::While { condition, body: while_body, .. } => {
            let cond_expr = condition.clone();
            let loop_body = while_body.clone();
            body.while_loop(
                |b| emit_expr(b, nctx, &cond_expr),
                |b| emit_body(b, nctx, &loop_body),
            );
        }
        Stmt::For { binding, iter, body: for_body } => {
            let arr = emit_expr(body, nctx, iter);
            let binding = binding.clone();
            let loop_body = for_body.clone();
            body.for_each(&binding, arr, |b| emit_body(b, nctx, &loop_body));
        }
        Stmt::Break => body.break_loop(),
        Stmt::Continue => body.continue_loop(),
        Stmt::Assign { name, value } => {
            let val = emit_expr(body, nctx, value);
            body.assign_name(name, val);
        }
        Stmt::FieldAssign { target, field, value } => {
            let var_name = match target {
                Expr::Ident(name) => name.as_str(),
                Expr::SelfRef => "self",
                _ => return,
            };
            let val = emit_expr(body, nctx, value);
            body.field_assign(var_name, field, val);
        }
        Stmt::LetResult { name, err_name, value } => {
            if let Expr::Call { target, args } = value {
                if let Expr::Ident(fn_name) = target.as_ref() {
                    let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, nctx, a)).collect();
                    emit_let_result(body, nctx, name, err_name, fn_name, &arg_vals);
                }
            }
        }
        Stmt::ReturnErr { name, .. } => {
            body.return_err(name);
        }
        Stmt::Wait { names, failed_name, kind } => {
            emit_wait(body, nctx, names, failed_name, kind);
        }
    }
}

pub fn emit_expr(body: &mut Body, nctx: &NativeCtx, expr: &Expr) -> Value {
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
                && nctx.infer_type(left, body) == RocaType::String;
            let l = emit_expr(body, nctx, left);
            let r = emit_expr(body, nctx, right);
            let is_float = body.is_number(l);
            let result = match op {
                BinOp::Add if is_float => body.add(l, r),
                BinOp::Add => body.string_concat(l, r),
                BinOp::Sub => body.sub(l, r),
                BinOp::Mul => body.mul(l, r),
                BinOp::Div => body.div(l, r),
                BinOp::Eq if is_float => body.eq(l, r),
                BinOp::Eq => body.string_eq(l, r),
                BinOp::Neq if is_float => body.neq(l, r),
                BinOp::Neq => body.string_neq(l, r),
                BinOp::Lt => body.lt(l, r),
                BinOp::Gt => body.gt(l, r),
                BinOp::Lte => body.lte(l, r),
                BinOp::Gte => body.gte(l, r),
                BinOp::And => body.and(l, r),
                BinOp::Or => body.or(l, r),
            };
            if l_is_temp_string { body.release_rc(l); }
            result
        }
        Expr::Not(inner) => {
            let val = emit_expr(body, nctx, inner);
            body.not(val)
        }
        Expr::StructLit { name, fields } => emit_struct_lit(body, nctx, name, fields),
        Expr::Call { target, args } => emit_call(body, nctx, target, args),
        Expr::Array(elements) => {
            let vals: Vec<Value> = elements.iter().map(|e| emit_expr(body, nctx, e)).collect();
            body.array(&vals)
        }
        Expr::Index { target, index } => {
            let arr = emit_expr(body, nctx, target);
            let idx = emit_expr(body, nctx, index);
            body.index(arr, idx)
        }
        Expr::Closure { params, body: closure_body } => {
            let name = format!("__closure_{}_{}", params.len(), closure_hash(params, closure_body));
            body.closure_ref(&name)
        }
        Expr::StringInterp(parts) => {
            let converted: Vec<StringPart> = parts.iter().map(|p| match p {
                AstStringPart::Literal(s) => StringPart::Lit(s.clone()),
                AstStringPart::Expr(e) => StringPart::Expr(emit_expr(body, nctx, e)),
            }).collect();
            body.string_interp(&converted)
        }
        Expr::Match { value, arms } => emit_match(body, nctx, value, arms),
        Expr::FieldAccess { target, field } => emit_field_access(body, nctx, target, field),
        Expr::EnumVariant { enum_name: _, variant, args } => {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, nctx, a)).collect();
            body.enum_variant("", variant, &arg_vals)
        }
        Expr::Await(inner) => emit_expr(body, nctx, inner),
    }
}

fn emit_call(body: &mut Body, nctx: &NativeCtx, target: &Expr, args: &[Expr]) -> Value {
    // Method call: obj.method(args)
    if let Expr::FieldAccess { target: obj, field: method } = target {
        return emit_method_call(body, nctx, obj, method, args);
    }

    if let Expr::Ident(name) = target {
        // log() builtin
        if name == "log" {
            if let Some(arg) = args.first() {
                let val = emit_expr(body, nctx, arg);
                emit_log(body, val);
            }
            return body.bool_val(false);
        }

        let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, nctx, a)).collect();

        // Known function call — check for crash handler
        if body.has_func(name) {
            if let Some(handler) = nctx.get_crash_handler(name).cloned() {
                return emit_crash_call(body, name, &arg_vals, &handler);
            }
            return body.call(name, &arg_vals);
        }

        // Closure call (variable holding a function pointer)
        return body.call_closure(name, &arg_vals);
    }
    body.null()
}

fn emit_method_call(body: &mut Body, nctx: &NativeCtx, target: &Expr, method: &str, args: &[Expr]) -> Value {
    // Static/type-level dispatch: Type.method(args)
    if let Expr::Ident(type_name) = target {
        let qualified = format!("{}.{}", type_name, method);
        let has_func = body.has_func(&qualified);
        if has_func {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, nctx, a)).collect();
            return body.call(&qualified, &arg_vals);
        }
        if nctx.is_enum_variant(type_name, method) {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, nctx, a)).collect();
            return body.enum_variant(type_name, method, &arg_vals);
        }
    }

    // Inline map/filter
    if (method == "map" || method == "filter") && !args.is_empty() {
        if let Expr::Closure { params, body: closure_body } = &args[0] {
            let arr = emit_expr(body, nctx, target);
            let param_name = params.first().cloned().unwrap_or_default();
            let closure_body = *closure_body.clone();
            return if method == "map" {
                emit_inline_map(body, nctx, arr, &param_name, |b, nc| emit_expr(b, nc, &closure_body))
            } else {
                emit_inline_filter(body, nctx, arr, &param_name, |b, nc| emit_expr(b, nc, &closure_body))
            };
        }
    }

    // Track temp heap values for chained method calls (strings and arrays)
    let target_type = if !matches!(target, Expr::Ident(_) | Expr::String(_)) {
        Some(nctx.infer_type(target, body))
    } else { None };
    let target_is_temp_heap = target_type.as_ref()
        .map_or(false, |t| matches!(t, RocaType::String | RocaType::Array(_)));

    let obj = emit_expr(body, nctx, target);
    let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, nctx, a)).collect();
    let result = emit_stdlib_dispatch(body, obj, method, &arg_vals);

    if target_is_temp_heap {
        match target_type.as_ref() {
            Some(RocaType::Array(_)) => { body.call_void("__free_array", &[obj]); }
            _ => { body.release_rc(obj); }
        }
    }
    result
}

fn emit_field_access(body: &mut Body, nctx: &NativeCtx, target: &Expr, field: &str) -> Value {
    // Enum unit variant: Token.Plus
    if let Expr::Ident(name) = target {
        if nctx.is_enum_variant(name, field) {
            return body.enum_variant(name, field, &[]);
        }
    }

    let var_name = match target {
        Expr::Ident(name) => Some(name.as_str()),
        Expr::SelfRef => Some("self"),
        _ => None,
    };

    if let Some(var_name) = var_name {
        let obj = emit_expr(body, nctx, target);
        return body.field_access_on(var_name, obj, field);
    }

    let obj = emit_expr(body, nctx, target);
    emit_stdlib_dispatch(body, obj, field, &[])
}

fn emit_struct_lit(body: &mut Body, nctx: &NativeCtx, name: &str, fields: &[(String, Expr)]) -> Value {
    let field_vals: Vec<(&str, Value)> = fields.iter()
        .map(|(n, e)| (n.as_str(), emit_expr(body, nctx, e)))
        .collect();

    let ptr = body.struct_lit_checked(name, &field_vals);

    if let Some(defs) = nctx.struct_defs(name) {
        let has_constraints = defs.iter().any(|d| !d.constraints.is_empty());
        if has_constraints {
            emit_struct_field_constraints(body, ptr, name, defs);
        }
    }

    ptr
}

fn emit_match(body: &mut Body, nctx: &NativeCtx, value: &Expr, arms: &[roca::MatchArm]) -> Value {
    let scrutinee = emit_expr(body, nctx, value);

    let default_arm = arms.iter().find(|a| a.pattern.is_none());
    let remaining: Vec<_> = arms.iter().filter(|a| a.pattern.is_some()).collect();

    // Pre-evaluate pattern values (but NOT results - they may depend on bindings)
    let mut match_arms = Vec::new();
    for arm in &remaining {
        match &arm.pattern {
            Some(roca::MatchPattern::Value(pattern)) => {
                let pat = emit_expr(body, nctx, pattern);
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

    // Infer result type from arms
    let result_type = if let Some(ref def) = default_expr {
        let kind = nctx.infer_type(def, body);
        if kind == RocaType::Number { RocaType::Number } else { RocaType::Unknown }
    } else if let Some(first) = match_arms.first() {
        let expr = match first {
            CompiledArm::Value { value_expr, .. } => value_expr,
            CompiledArm::Variant { value_expr, .. } => value_expr,
        };
        let kind = nctx.infer_type(expr, body);
        if kind == RocaType::Number { RocaType::Number } else { RocaType::Unknown }
    } else if body.is_number(scrutinee) { RocaType::Number }
    else { RocaType::Unknown };

    body.match_lazy(scrutinee, &match_arms, &default_expr,
        |b, e| emit_expr(b, nctx, e), result_type)
}

/// Intermediate representation for match arms with deferred result evaluation.
pub enum CompiledArm {
    Value { pattern: Value, value_expr: roca::Expr },
    Variant { variant: String, bindings: Vec<String>, value_expr: roca::Expr },
}

impl MatchArmLazy<roca::Expr> for CompiledArm {
    fn kind(&self) -> LazyArmKind<'_, roca::Expr> {
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

/// Destructure error tuple: let {name, err_name} = call(fn_name, args)
fn emit_let_result(body: &mut Body, nctx: &NativeCtx, name: &str, err_name: &str, fn_name: &str, args: &[Value]) {
    let results = body.call_multi(fn_name, args);
    if results.len() >= 2 {
        let val = results[0];
        let err = results[1];
        let kind = if body.is_number(val) {
            RocaType::Number
        } else if let Some(k) = nctx.func_return_kinds.get(fn_name) {
            k.clone()
        } else {
            // Unknown heap return — default to RcRelease so it gets freed
            RocaType::String
        };
        body.const_var_typed(name, val, kind);
        body.const_var_typed(err_name, err, RocaType::Bool);
    } else {
        // Function not found or single-return — bind defaults so variables exist in scope
        let null_val = body.null();
        let false_val = body.bool_val(false);
        body.const_var_typed(name, null_val, RocaType::Unknown);
        body.const_var_typed(err_name, false_val, RocaType::Bool);
    }
}

fn emit_wait(body: &mut Body, nctx: &NativeCtx, names: &[String], failed_name: &str, kind: &roca::WaitKind) {
    match kind {
        roca::WaitKind::Single(expr) => {
            let kind = nctx.infer_type(expr, body);
            let val = emit_expr(body, nctx, expr);
            if !names.is_empty() {
                body.const_var_typed(&names[0], val, kind);
            }
            bind_failed(body, failed_name);
        }
        roca::WaitKind::All(exprs) => {
            let fn_names: Vec<String> = exprs.iter()
                .map(|e| format!("__wait_{}", super::compile::wait_expr_hash(e)))
                .collect();
            emit_wait_all(body, names, failed_name, &fn_names);
        }
        roca::WaitKind::First(exprs) => {
            let fn_names: Vec<String> = exprs.iter()
                .map(|e| format!("__wait_{}", super::compile::wait_expr_hash(e)))
                .collect();
            emit_wait_first(body, names, failed_name, &fn_names);
        }
    }
}

/// Wait for all async functions, bind results to names.
fn emit_wait_all(body: &mut Body, names: &[String], failed_name: &str, fn_names: &[String]) {
    let (arr, count) = build_wait_fn_array(body, fn_names);
    let results = body.call("__wait_all", &[arr, count]);
    for (i, name) in names.iter().enumerate() {
        let idx = body.int(i as i64);
        let val = body.call("__array_get_f64", &[results, idx]);
        body.const_var_typed(name, val, RocaType::Number);
    }
    bind_failed(body, failed_name);
}

fn emit_wait_first(body: &mut Body, names: &[String], failed_name: &str, fn_names: &[String]) {
    let (arr, count) = build_wait_fn_array(body, fn_names);
    let val = body.call("__wait_first", &[arr, count]);
    if !names.is_empty() {
        body.const_var_typed(&names[0], val, RocaType::Number);
    }
    bind_failed(body, failed_name);
}

/// Build a function-pointer array from pre-compiled wait function names.
fn build_wait_fn_array(body: &mut Body, fn_names: &[String]) -> (Value, Value) {
    let arr = body.call("__array_new", &[]);
    for name in fn_names {
        if let Some(ptr) = body.func_addr(name) {
            body.call_void("__array_push_str", &[arr, ptr]);
        }
    }
    let count = body.int(fn_names.len() as i64);
    (arr, count)
}

fn bind_failed(body: &mut Body, name: &str) {
    let false_val = body.bool_val(false);
    body.const_var_typed(name, false_val, RocaType::Bool);
}

/// Simple hash for identifying closures by their AST structure.
pub fn closure_hash(params: &[String], body: &Expr) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    for p in params { p.hash(&mut h); }
    h.finish() ^ super::compile::expr_debug_hash(body)
}

// ─── Roca Language Semantics ─────────────────────────
// These use Body's public API to implement Roca-specific dispatch.

/// Log dispatch — routes to print_f64/print_bool/print based on type.
fn emit_log(body: &mut Body, val: Value) {
    if body.is_number(val) {
        body.call_void("__print_f64", &[val]);
    } else if body.is_bool(val) {
        body.call_void("__print_bool", &[val]);
    } else {
        body.call_void("__print", &[val]);
    }
}

/// Stdlib method dispatch — resolves instance methods to runtime function calls.
fn emit_stdlib_dispatch(body: &mut Body, obj: Value, method: &str, args: &[Value]) -> Value {
    match method {
        "trim" => return body.call("__string_trim", &[obj]),
        "toUpperCase" => return body.call("__string_to_upper", &[obj]),
        "toLowerCase" => return body.call("__string_to_lower", &[obj]),
        "includes" | "contains" => {
            let needle = args.first().copied().unwrap_or_else(|| body.null());
            let result = body.call("__string_includes", &[obj, needle]);
            return body.extend_bool(result);
        }
        "startsWith" => {
            let prefix = args.first().copied().unwrap_or_else(|| body.null());
            let result = body.call("__string_starts_with", &[obj, prefix]);
            return body.extend_bool(result);
        }
        "endsWith" => {
            let suffix = args.first().copied().unwrap_or_else(|| body.null());
            let result = body.call("__string_ends_with", &[obj, suffix]);
            return body.extend_bool(result);
        }
        "indexOf" => {
            let needle = args.first().copied().unwrap_or_else(|| body.null());
            return body.call("__string_index_of", &[obj, needle]);
        }
        "charCodeAt" => {
            let idx = args.first().copied().unwrap_or_else(|| body.null());
            let idx_i = body.to_i64(idx);
            return body.call("__string_char_code_at", &[obj, idx_i]);
        }
        "charAt" => {
            let idx = args.first().copied().unwrap_or_else(|| body.null());
            let idx_i = body.to_i64(idx);
            return body.call("__string_char_at", &[obj, idx_i]);
        }
        "slice" => {
            let start = args.first().copied().unwrap_or_else(|| body.null());
            let end = args.get(1).copied().unwrap_or_else(|| body.null());
            let start_i = body.to_i64(start);
            let end_i = body.to_i64(end);
            return body.call("__string_slice", &[obj, start_i, end_i]);
        }
        "split" => {
            let delim = args.first().copied().unwrap_or_else(|| body.null());
            return body.call("__string_split", &[obj, delim]);
        }
        "join" => {
            let sep = args.first().copied().unwrap_or_else(|| body.null());
            return body.call("__array_join", &[obj, sep]);
        }
        "toString" => {
            if body.is_number(obj) {
                return body.call("__string_from_f64", &[obj]);
            }
            return obj;
        }
        "push" => {
            let val = args.first().copied().unwrap_or_else(|| body.null());
            body.array_push(obj, val);
            return obj;
        }
        "length" | "len" => {
            let kind = if body.is_number(obj) { RocaType::Number } else { RocaType::Unknown };
            return body.length_with_kind(obj, kind);
        }
        _ => {}
    }
    obj
}

// ─── Constraint Validation ───────────────────────────

/// Validate parameter constraints at function entry.
pub fn emit_param_constraints(body: &mut Body, params: &[roca::Param]) {
    for param in params {
        if param.constraints.is_empty() { continue; }
        let val = body.var(&param.name);
        let is_string = matches!(param.type_ref, roca::TypeRef::String);
        emit_constraints(body, val, is_string, &param.name, &param.constraints);
    }
}

/// Validate constraints on struct fields after construction.
pub fn emit_struct_field_constraints(
    body: &mut Body,
    ptr: Value,
    struct_name: &str,
    field_defs: &[roca::Field],
) {
    for field_def in field_defs {
        if field_def.constraints.is_empty() { continue; }
        let layout_idx = body.struct_field_index(struct_name, &field_def.name);
        if let Some(idx) = layout_idx {
            let is_string = matches!(field_def.type_ref, roca::TypeRef::String);
            let get_fn = if is_string { "__struct_get_ptr" } else { "__struct_get_f64" };
            let field_idx = body.int(idx as i64);
            let val = body.call(get_fn, &[ptr, field_idx]);
            emit_constraints(body, val, is_string, &field_def.name, &field_def.constraints);
        }
    }
}

fn emit_constraints(
    body: &mut Body,
    val: Value,
    is_string: bool,
    name: &str,
    constraints: &[roca::Constraint],
) {
    for constraint in constraints {
        match constraint {
            roca::Constraint::Min(n) if !is_string => {
                let min_val = body.number(*n);
                let cond = body.float_lt(val, min_val);
                emit_constraint_trap(body, cond, name, &format!("must be >= {}", n));
            }
            roca::Constraint::Max(n) if !is_string => {
                let max_val = body.number(*n);
                let cond = body.float_gt(val, max_val);
                emit_constraint_trap(body, cond, name, &format!("must be <= {}", n));
            }
            roca::Constraint::Min(n) | roca::Constraint::MinLen(n) if is_string => {
                let len = body.call("__string_len", &[val]);
                let min_val = body.int(*n as i64);
                let cond = body.int_lt(len, min_val);
                emit_constraint_trap(body, cond, name, &format!("min length {}", n));
            }
            roca::Constraint::Max(n) | roca::Constraint::MaxLen(n) if is_string => {
                let len = body.call("__string_len", &[val]);
                let max_val = body.int(*n as i64);
                let cond = body.int_gt(len, max_val);
                emit_constraint_trap(body, cond, name, &format!("max length {}", n));
            }
            roca::Constraint::Contains(s) => {
                let needle = body.cstr(s);
                let result = body.call("__string_includes", &[val, needle]);
                let ext = body.extend_bool(result);
                let one = body.int(1);
                let not_result = body.int_sub(one, ext);
                emit_constraint_trap(body, not_result, name, &format!("must contain \"{}\"", s));
            }
            _ => {}
        }
    }
}

fn emit_constraint_trap(body: &mut Body, cond: Value, field: &str, msg: &str) {
    let err_msg = format!("{}: {}", field, msg);
    body.if_then(cond, |b| {
        let msg_ptr = b.cstr(&err_msg);
        b.call_void("__constraint_panic", &[msg_ptr]);
        b.return_default_err();
    });
}

// ─── Crash Handling ─────────────────────────────────

fn emit_crash_call(
    body: &mut Body,
    name: &str,
    args: &[Value],
    handler: &CrashHandlerKind,
) -> Value {
    let chain = match handler {
        CrashHandlerKind::Simple(chain) => chain.clone(),
        CrashHandlerKind::Detailed { default, .. } => {
            default.clone().unwrap_or_else(|| vec![CrashStep::Halt])
        }
    };

    let retry = chain.iter().find_map(|s| {
        if let CrashStep::Retry { attempts, delay_ms } = s {
            Some((*attempts, *delay_ms))
        } else { None }
    });

    let arg_is_number: Vec<bool> = args.iter().map(|v| body.is_number(*v)).collect();

    for (i, &arg) in args.iter().enumerate() {
        let var_name = format!("__crash_arg_{}", i);
        if arg_is_number[i] {
            body.let_var_typed(&var_name, arg, RocaType::Number);
        } else {
            body.let_var(&var_name, arg);
        }
    }

    fn reload_args(body: &mut Body, count: usize) -> Vec<Value> {
        (0..count).map(|i| body.var(&format!("__crash_arg_{}", i))).collect()
    }

    let call_args = reload_args(body, args.len());
    let results = body.call_multi(name, &call_args);
    if results.len() < 2 {
        return if results.is_empty() { body.null() } else { results[0] };
    }

    let result_is_number = body.is_number(results[0]);
    if result_is_number {
        body.let_var_typed("__crash_val", results[0], RocaType::Number);
    } else {
        body.let_var("__crash_val", results[0]);
    }
    body.let_var_typed("__crash_err", results[1], RocaType::Bool);

    if let Some((attempts, delay_ms)) = retry {
        let counter_init = body.int(1);
        body.let_var("__crash_counter", counter_init);

        let first_err = body.var("__crash_err");
        let arg_count = args.len();
        let name_c = name.to_string();
        body.if_then(first_err, move |b| {
            let max = b.int(attempts as i64);
            let name_c2 = name_c.clone();
            b.while_loop(
                |b| {
                    let counter = b.var("__crash_counter");
                    let has_more = b.int_lt(counter, max);
                    let err = b.var("__crash_err");
                    let err_ext = b.extend_bool(err);
                    b.and(has_more, err_ext)
                },
                move |b| {
                    if delay_ms > 0 {
                        let ms = b.number(delay_ms as f64);
                        b.call_void("__sleep", &[ms]);
                    }

                    let retry_args = reload_args(b, arg_count);
                    let retry_results = b.call_multi(&name_c2, &retry_args);
                    if retry_results.len() >= 2 {
                        b.assign_name("__crash_val", retry_results[0]);
                        b.assign_name("__crash_err", retry_results[1]);
                    }

                    let cur = b.var("__crash_counter");
                    let one = b.int(1);
                    let next = b.int_add(cur, one);
                    b.assign_name("__crash_counter", next);
                },
            );
        });
    }

    let final_err = body.var("__crash_err");
    let val_loaded = body.var("__crash_val");
    if result_is_number {
        body.let_var_typed("__crash_result", val_loaded, RocaType::Number);
    } else {
        body.let_var("__crash_result", val_loaded);
    }

    let chain_no_retry: Vec<_> = chain.into_iter()
        .filter(|s| !matches!(s, CrashStep::Retry { .. }))
        .collect();

    body.if_then(final_err, |b| {
        emit_crash_chain(b, &chain_no_retry);
    });

    body.var("__crash_result")
}

fn emit_crash_chain(body: &mut Body, chain: &[CrashStep]) {
    for step in chain {
        match step {
            CrashStep::Log => {
                let msg = body.cstr("error");
                body.call_void("__print", &[msg]);
            }
            CrashStep::Halt => {
                body.return_default_err();
                return;
            }
            CrashStep::Panic => {
                body.panic();
                return;
            }
            CrashStep::Skip => {}
            CrashStep::Fallback(_expr) => {}
            CrashStep::Retry { .. } => {}
        }
    }
}

// ─── Inline Map/Filter ──────────────────────────────

fn emit_inline_map(
    body: &mut Body,
    nctx: &NativeCtx,
    arr: Value,
    binding: &str,
    body_fn: impl FnOnce(&mut Body, &NativeCtx) -> Value,
) -> Value {
    let result_arr = body.call("__array_new", &[]);
    body.let_var("__map_result", result_arr);
    let binding = binding.to_string();

    body.for_each(&binding, arr, |b| {
        let mapped = body_fn(b, nctx);
        let res = b.var("__map_result");
        b.array_push(res, mapped);
    });

    body.var("__map_result")
}

fn emit_inline_filter(
    body: &mut Body,
    nctx: &NativeCtx,
    arr: Value,
    binding: &str,
    body_fn: impl FnOnce(&mut Body, &NativeCtx) -> Value,
) -> Value {
    let result_arr = body.call("__array_new", &[]);
    body.let_var("__filter_result", result_arr);
    let binding = binding.to_string();

    body.for_each(&binding, arr, |b| {
        let cond = body_fn(b, nctx);
        let elem = b.var(&binding);
        let res = b.var("__filter_result");
        b.if_then(cond, |b| {
            b.array_push(res, elem);
        });
    });

    body.var("__filter_result")
}
