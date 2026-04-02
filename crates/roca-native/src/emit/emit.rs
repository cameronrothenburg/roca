//! AST walker — translates Roca AST to Body method calls.
//! Zero IR imports. Every expression is 1-3 lines, every statement is 2-5 lines.
//!
//! Roca language semantics (stdlib dispatch, log, inline map/filter, constraint
//! validation) live here as free functions that compose Body's IR primitives.

use roca_ast::{self as roca, Expr, Stmt, BinOp, StringPart as AstStringPart};
use roca_ast::crash::{CrashHandlerKind, CrashStep};
use roca_cranelift::api::{Body, Value, StringPart, MatchArmLazy, LazyArmKind, VarSlot};
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
                emit_log(body, val);
            }
            return body.bool_val(false);
        }

        let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();

        // Known function call — check for crash handler
        if body.has_func(name) {
            if let Some(handler) = body.get_crash_handler(name) {
                return emit_crash_call(body, name, &arg_vals, &handler);
            }
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
                emit_inline_map(body, arr, &param_name, |b| emit_expr(b, &closure_body))
            } else {
                emit_inline_filter(body, arr, &param_name, |b| emit_expr(b, &closure_body))
            };
        }
    }

    // Track temp strings for chained method calls
    let target_is_temp_string = !matches!(target, Expr::Ident(_) | Expr::String(_))
        && body.infer_type(target) == RocaType::String;

    let obj = emit_expr(body, target);
    let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();
    let result = emit_stdlib_dispatch(body, obj, method, &arg_vals);

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
    emit_stdlib_dispatch(body, obj, field, &[])
}

fn emit_struct_lit(body: &mut Body, name: &str, fields: &[(String, Expr)]) -> Value {
    let field_vals: Vec<(&str, Value)> = fields.iter()
        .map(|(n, e)| (n.as_str(), emit_expr(body, e)))
        .collect();

    let defs = body.struct_defs(name);
    let ptr = body.struct_lit_checked(name, &field_vals, defs.as_deref());

    // Validate constraints on struct fields
    if let Some(ref defs) = defs {
        let has_constraints = defs.iter().any(|d| !d.constraints.is_empty());
        if has_constraints {
            emit_struct_field_constraints(body, ptr, name, defs);
        }
    }

    ptr
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

// ─── Roca Language Semantics ─────────────────────────
// These use Body's IR primitives to implement Roca-specific dispatch.

/// Log dispatch — routes to print_f64/print_bool/print based on type.
fn emit_log(body: &mut Body, val: Value) {
    if body.is_number(val) {
        body.call_void("__print_f64", &[val]);
    } else if body.value_type(val) == cranelift_codegen::ir::types::I8 {
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
    // Fallback: unknown method — return object as-is
    obj
}

// ─── Constraint Validation ───────────────────────────
// Roca-specific constraint checks built from Body's IR primitives.

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
            let field_idx = body.i64_const(idx as i64);
            let val = body.call(get_fn, &[ptr, field_idx]);
            emit_constraints(body, val, is_string, &field_def.name, &field_def.constraints);
        }
    }
}

/// Validate constraints on a single value.
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
                let cond = body.f64_lt(val, min_val);
                emit_constraint_trap(body, cond, name, &format!("must be >= {}", n));
            }
            roca::Constraint::Max(n) if !is_string => {
                let max_val = body.number(*n);
                let cond = body.f64_gt(val, max_val);
                emit_constraint_trap(body, cond, name, &format!("must be <= {}", n));
            }
            roca::Constraint::Min(n) | roca::Constraint::MinLen(n) if is_string => {
                let len = body.call("__string_len", &[val]);
                let min_val = body.i64_const(*n as i64);
                let cond = body.i64_slt(len, min_val);
                emit_constraint_trap(body, cond, name, &format!("min length {}", n));
            }
            roca::Constraint::Max(n) | roca::Constraint::MaxLen(n) if is_string => {
                let len = body.call("__string_len", &[val]);
                let max_val = body.i64_const(*n as i64);
                let cond = body.i64_sgt(len, max_val);
                emit_constraint_trap(body, cond, name, &format!("max length {}", n));
            }
            roca::Constraint::Contains(s) => {
                let needle = body.static_str(s);
                let result = body.call("__string_includes", &[val, needle]);
                let ext = body.extend_bool(result);
                let one = body.i64_const(1);
                let not_result = body.i64_sub(one, ext);
                emit_constraint_trap(body, not_result, name, &format!("must contain \"{}\"", s));
            }
            _ => {}
        }
    }
}

/// Emit a constraint violation trap: if cond is non-zero, print error and return default.
fn emit_constraint_trap(body: &mut Body, cond: Value, field: &str, msg: &str) {
    let err_msg = format!("{}: {}", field, msg);
    body.if_then(cond, |b| {
        let msg_ptr = b.static_str(&err_msg);
        b.call_void("__constraint_panic", &[msg_ptr]);
        b.return_default_err();
    });
}

// ─── Crash Handling ─────────────────────────────────
// Roca crash strategies built from Body's IR primitives.

/// Call a function with crash handler — retry loop + error chain dispatch.
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

    // Track which args are numbers for proper slot reload
    let arg_is_number: Vec<bool> = args.iter().map(|v| body.is_number(*v)).collect();
    let arg_slots: Vec<VarSlot> = args.iter().map(|v| body.alloc_slot(*v)).collect();

    // Helper: reload args from slots with correct types
    fn reload_args(body: &mut Body, slots: &[VarSlot], is_number: &[bool]) -> Vec<Value> {
        slots.iter().zip(is_number).map(|(s, &n)| {
            body.load_slot_if_number(*s, n)
        }).collect()
    }

    // Initial call
    let call_args = reload_args(body, &arg_slots, &arg_is_number);
    let results = body.call_multi(name, &call_args);
    if results.len() < 2 {
        return if results.is_empty() { body.null() } else { results[0] };
    }

    let result_is_number = body.is_number(results[0]);
    let value_slot = body.alloc_slot(results[0]);
    let err_slot = body.alloc_slot(results[1]);

    // Retry loop
    if let Some((attempts, delay_ms)) = retry {
        let counter_init = body.i64_const(1);
        let counter_slot = body.alloc_slot(counter_init);

        let first_err = body.load_slot_bool(err_slot);
        let arg_slots_c = arg_slots.clone();
        let arg_is_number_c = arg_is_number.clone();
        let name_c = name.to_string();
        body.if_then(first_err, move |b| {
            let max = b.i64_const(attempts as i64);
            let name_c2 = name_c.clone();
            let arg_slots_c2 = arg_slots_c.clone();
            let arg_is_number_c2 = arg_is_number_c.clone();
            b.while_loop(
                |b| {
                    let counter = b.load_slot(counter_slot);
                    let has_more = b.i64_slt(counter, max);
                    let err = b.load_slot_bool(err_slot);
                    let err_ext = b.extend_bool(err);
                    // Continue while has_more AND err
                    b.binop(&BinOp::And, has_more, err_ext)
                },
                move |b| {
                    if delay_ms > 0 {
                        let ms = b.number(delay_ms as f64);
                        b.call_void("__sleep", &[ms]);
                    }

                    let retry_args = reload_args(b, &arg_slots_c2, &arg_is_number_c2);
                    let retry_results = b.call_multi(&name_c2, &retry_args);
                    if retry_results.len() >= 2 {
                        b.store_slot(value_slot, retry_results[0]);
                        b.store_slot(err_slot, retry_results[1]);
                    }

                    let cur = b.load_slot(counter_slot);
                    let one = b.i64_const(1);
                    let next = b.i64_add(cur, one);
                    b.store_slot(counter_slot, next);
                },
            );
        });
    }

    // Error handler dispatch: if err, run crash chain; else return value
    let final_err = body.load_slot_bool(err_slot);
    let val_loaded = body.load_slot_if_number(value_slot, result_is_number);
    let result_slot = body.alloc_slot(val_loaded);

    let chain_no_retry: Vec<_> = chain.into_iter()
        .filter(|s| !matches!(s, CrashStep::Retry { .. }))
        .collect();

    body.if_then(final_err, |b| {
        emit_crash_chain(b, &chain_no_retry, result_slot);
    });

    body.load_slot_if_number(result_slot, result_is_number)
}

/// Emit crash chain steps (log, halt, panic, skip, fallback) inside an error branch.
fn emit_crash_chain(body: &mut Body, chain: &[CrashStep], result_slot: VarSlot) {
    for step in chain {
        match step {
            CrashStep::Log => {
                let msg = body.static_str("error");
                body.call_void("__print", &[msg]);
            }
            CrashStep::Halt => {
                body.return_default_err();
                return;
            }
            CrashStep::Panic => {
                body.trap(1);
                return;
            }
            CrashStep::Skip => {
                // Skip: default value already in result_slot, continue
            }
            CrashStep::Fallback(_expr) => {
                // TODO: emit fallback expression
            }
            CrashStep::Retry { .. } => {} // handled above
        }
    }
}

// ─── Inline Map/Filter ──────────────────────────────
// Array iteration patterns built from Body's for_each + slot primitives.

/// Inline map: iterate arr, apply body_fn to each element, collect results.
fn emit_inline_map(
    body: &mut Body,
    arr: Value,
    binding: &str,
    body_fn: impl FnOnce(&mut Body) -> Value,
) -> Value {
    let result_arr = body.call("__array_new", &[]);
    let result_slot = body.alloc_slot(result_arr);
    let binding = binding.to_string();

    body.for_each(&binding, arr, |b| {
        let mapped = body_fn(b);
        let res = b.load_slot(result_slot);
        b.array_push(res, mapped);
    });

    body.load_slot(result_slot)
}

/// Inline filter: iterate arr, keep elements where body_fn returns truthy.
fn emit_inline_filter(
    body: &mut Body,
    arr: Value,
    binding: &str,
    body_fn: impl FnOnce(&mut Body) -> Value,
) -> Value {
    let result_arr = body.call("__array_new", &[]);
    let result_slot = body.alloc_slot(result_arr);
    let binding = binding.to_string();

    body.for_each(&binding, arr, |b| {
        let cond = body_fn(b);
        let elem = b.var(&binding);
        let res = b.load_slot(result_slot);
        b.if_then(cond, |b| {
            b.array_push(res, elem);
        });
    });

    body.load_slot(result_slot)
}
