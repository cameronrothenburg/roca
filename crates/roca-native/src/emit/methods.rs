//! Method calls, struct/enum construction, crash handlers, and inline map/filter.

use cranelift_codegen::ir::{self, types, InstBuilder};

use roca_ast::{self as roca, Expr, crash::{CrashHandlerKind, CrashStep}};
use roca_cranelift::api::{Body, Value};
use roca_cranelift::builder::{FuncRef, VarSlot};
use roca_cranelift::context::{StructLayout, ValKind as RocaType};
use super::helpers::{
    infer_kind, emit_scope_cleanup, target_kind, first_arg_or_null,
    emit_array_push, emit_struct_set, emit_length,
};
use super::expr::emit_expr;

/// Map a raw Cranelift IR type back to its closest RocaType.
fn roca_type_for_ir(ty: ir::Type) -> RocaType {
    if ty == types::F64 { RocaType::Number }
    else if ty == types::I8 { RocaType::Bool }
    else { RocaType::Unknown }
}

/// Emit constraint validation guards for function parameters at entry.
pub fn emit_param_constraints(body: &mut Body, params: &[roca::Param]) {
    for param in params {
        if param.constraints.is_empty() { continue; }
        let var = match body.ctx.get_var(&param.name) {
            Some(v) => v.clone(),
            None => continue,
        };
        let val = body.ir.raw().ins().stack_load(var.cranelift_type, var.slot, 0);
        let is_string = matches!(param.type_ref, roca::TypeRef::String);
        emit_value_constraints(body, val, is_string, &param.name, &param.constraints);
    }
}

/// Shared constraint guard emission for a pre-loaded value.
/// Used by both param constraints and struct field constraints.
pub fn emit_value_constraints(
    body: &mut Body,
    val: Value,
    is_string: bool,
    name: &str,
    constraints: &[roca::Constraint],
) {
    for constraint in constraints {
        match constraint {
            roca::Constraint::Min(n) if !is_string => {
                let min_val = body.ir.const_number(*n);
                let cmp = body.ir.raw().ins().fcmp(ir::condcodes::FloatCC::LessThan, val, min_val);
                let cmp_ext = body.ir.extend_bool(cmp);
                emit_constraint_trap(body, cmp_ext, name, &format!("must be >= {}", n));
            }
            roca::Constraint::Max(n) if !is_string => {
                let max_val = body.ir.const_number(*n);
                let cmp = body.ir.raw().ins().fcmp(ir::condcodes::FloatCC::GreaterThan, val, max_val);
                let cmp_ext = body.ir.extend_bool(cmp);
                emit_constraint_trap(body, cmp_ext, name, &format!("must be <= {}", n));
            }
            roca::Constraint::Min(n) | roca::Constraint::MinLen(n) if is_string => {
                if let Some(&len_fn) = body.ctx.get_func("__string_len") {
                    let len = body.ir.call(len_fn, &[val]);
                    let min_val = body.ir.const_i64(*n as i64);
                    let cmp = body.ir.raw().ins().icmp(ir::condcodes::IntCC::SignedLessThan, len, min_val);
                    let cmp_ext = body.ir.extend_bool(cmp);
                    emit_constraint_trap(body, cmp_ext, name, &format!("min length {}", n));
                }
            }
            roca::Constraint::Max(n) | roca::Constraint::MaxLen(n) if is_string => {
                if let Some(&len_fn) = body.ctx.get_func("__string_len") {
                    let len = body.ir.call(len_fn, &[val]);
                    let max_val = body.ir.const_i64(*n as i64);
                    let cmp = body.ir.raw().ins().icmp(ir::condcodes::IntCC::SignedGreaterThan, len, max_val);
                    let cmp_ext = body.ir.extend_bool(cmp);
                    emit_constraint_trap(body, cmp_ext, name, &format!("max length {}", n));
                }
            }
            roca::Constraint::Contains(s) => {
                let needle = body.ir.leak_cstr(s);
                if let Some(&includes) = body.ctx.get_func("__string_includes") {
                    let result = body.ir.call(includes, &[val, needle]);
                    let not_result = {
                        let ext = body.ir.extend_bool(result);
                        let one = body.ir.const_i64(1);
                        body.ir.isub(one, ext)
                    };
                    emit_constraint_trap(body, not_result, name, &format!("must contain \"{}\"", s));
                }
            }
            _ => {}
        }
    }
}

pub fn emit_method_call(body: &mut Body, target: &Expr, method: &str, args: &[Expr]) -> Value {
    // Enum data variant constructor: Token.Number(42)
    if let Expr::Ident(name) = target {
        if body.ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&method.to_string())) {
            return emit_enum_variant(body, method, args);
        }
    }

    // Struct static method / extern contract method call: Counter.current(c), Fs.readFile(path)
    if let Expr::Ident(type_name) = target {
        let qualified = format!("{}.{}", type_name, method);
        if let Some(&func_ref) = body.ctx.get_func(&qualified) {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();

            if let Some(handler) = body.ctx.crash_handlers.get(&qualified).cloned() {
                // Store args in slots for potential retry
                let arg_slots: Vec<_> = arg_vals.iter().map(|v| body.ir.alloc_var(*v)).collect();
                let arg_types: Vec<_> = arg_vals.iter().map(|v| body.ir.value_ir_type(*v)).collect();
                return emit_crash_call(body, func_ref, &arg_slots, &arg_types, &handler);
            }

            let results = body.ir.call_multi(func_ref, &arg_vals);
            if results.len() >= 2 { return results[0]; }
            if !results.is_empty() { return results[0]; }
        }
    }

    let kind = target_kind(target, &mut body.ctx);

    // Inline map/filter before evaluating target — they need the closure
    if (method == "map" || method == "filter") && !args.is_empty() {
        if let Expr::Closure { params, body: closure_body } = &args[0] {
            return emit_inline_map_filter(body, target, method, params, closure_body);
        }
    }

    // Detect chained method calls that produce intermediate strings
    let target_is_temp_string = !matches!(target, Expr::Ident(_) | Expr::String(_))
        && infer_kind(target, &body.ctx) == RocaType::String;

    let obj = emit_expr(body, target);

    let result = match method {
        "push" => {
            if let Some(arg) = args.first() {
                let val = emit_expr(body, arg);
                emit_array_push(body.ir, obj, val, &mut body.ctx);
            }
            body.ir.raw().ins().iconst(types::I8, 0)
        }
        "pop" => {
            if let Some(&get) = body.ctx.get_func("__array_get_f64") {
                if let Some(&len_fn) = body.ctx.get_func("__array_len") {
                    let len = body.ir.call(len_fn, &[obj]);
                    let one = body.ir.const_i64(1);
                    let last_idx = body.ir.isub(len, one);
                    return body.ir.call(get, &[obj, last_idx]);
                }
            }
            body.number(0.0)
        }
        "join" => {
            let sep = if let Some(arg) = args.first() {
                emit_expr(body, arg)
            } else {
                body.ir.leak_cstr(",")
            };
            if let Some(&f) = body.ctx.get_func("__array_join") {
                body.ir.call(f, &[obj, sep])
            } else { body.null() }
        }

        "includes" | "contains" => {
            let needle = first_arg_or_null(body, args);
            if let Some(&f) = body.ctx.get_func("__string_includes") {
                let result = body.ir.call(f, &[obj, needle]);
                body.ir.extend_bool(result)
            } else { body.null() }
        }
        "startsWith" => {
            let prefix = first_arg_or_null(body, args);
            if let Some(&f) = body.ctx.get_func("__string_starts_with") {
                let result = body.ir.call(f, &[obj, prefix]);
                body.ir.extend_bool(result)
            } else { body.null() }
        }
        "endsWith" => {
            let suffix = first_arg_or_null(body, args);
            if let Some(&f) = body.ctx.get_func("__string_ends_with") {
                let result = body.ir.call(f, &[obj, suffix]);
                body.ir.extend_bool(result)
            } else { body.null() }
        }
        "trim" => {
            if let Some(&f) = body.ctx.get_func("__string_trim") {
                body.ir.call(f, &[obj])
            } else { obj }
        }
        "toUpperCase" => {
            if let Some(&f) = body.ctx.get_func("__string_to_upper") {
                body.ir.call(f, &[obj])
            } else { obj }
        }
        "toLowerCase" => {
            if let Some(&f) = body.ctx.get_func("__string_to_lower") {
                body.ir.call(f, &[obj])
            } else { obj }
        }
        "slice" => {
            let start = args.first().map(|a| emit_expr(body, a))
                .unwrap_or_else(|| body.null());
            let end = args.get(1).map(|a| emit_expr(body, a))
                .unwrap_or_else(|| {
                    if let Some(&f) = body.ctx.get_func("__string_len") {
                        body.ir.call(f, &[obj])
                    } else { body.null() }
                });
            let start_i = body.ir.to_i64(start);
            let end_i = body.ir.to_i64(end);
            if let Some(&f) = body.ctx.get_func("__string_slice") {
                body.ir.call(f, &[obj, start_i, end_i])
            } else { obj }
        }
        "split" => {
            let delim = first_arg_or_null(body, args);
            if let Some(&f) = body.ctx.get_func("__string_split") {
                body.ir.call(f, &[obj, delim])
            } else { body.null() }
        }
        "charAt" => {
            let idx = first_arg_or_null(body, args);
            let idx_i = body.ir.to_i64(idx);
            if let Some(&f) = body.ctx.get_func("__string_char_at") {
                body.ir.call(f, &[obj, idx_i])
            } else { body.null() }
        }
        "charCodeAt" => {
            let idx = first_arg_or_null(body, args);
            let idx_i = body.ir.to_i64(idx);
            if let Some(&f) = body.ctx.get_func("__string_char_code_at") {
                body.ir.call(f, &[obj, idx_i])
            } else { body.number(0.0) }
        }
        "indexOf" => {
            let needle = first_arg_or_null(body, args);
            if let Some(&f) = body.ctx.get_func("__string_index_of") {
                body.ir.call(f, &[obj, needle])
            } else { body.number(-1.0) }
        }

        "len" | "length" => emit_length(body.ir, obj, kind, &mut body.ctx),
        "toString" => {
            let ty = body.ir.value_ir_type(obj);
            if ty == types::F64 {
                if let Some(&f) = body.ctx.get_func("__string_from_f64") {
                    body.ir.call(f, &[obj])
                } else { body.null() }
            } else {
                obj
            }
        }
        _ => obj,
    };

    // Free intermediate string from chained method calls
    if target_is_temp_string {
        if let Some(&f) = body.ctx.get_func("__rc_release") {
            body.ir.call_void(f, &[obj]);
        }
    }

    result
}

pub fn emit_struct_lit(body: &mut Body, name: &str, fields: &[(String, Expr)]) -> Value {
    if !body.ctx.struct_layouts.contains_key(name) {
        body.ctx.struct_layouts.insert(name.to_string(), StructLayout {
            fields: fields.iter().map(|(n, v)| (n.clone(), infer_kind(v, &body.ctx))).collect(),
        });
    }

    let num_fields = body.ir.const_i64(fields.len() as i64);
    let ptr = if let Some(f) = body.ctx.get_func("__struct_alloc") {
        body.ir.call(*f, &[num_fields])
    } else {
        return body.null();
    };

    let indices: Vec<usize> = {
        let layout = body.ctx.struct_layouts.get(name).unwrap();
        fields.iter().map(|(n, _)| layout.field_index(n).unwrap_or(0)).collect()
    };

    for (i, (_, field_expr)) in fields.iter().enumerate() {
        let val = emit_expr(body, field_expr);
        let idx_val = body.ir.const_i64(indices[i] as i64);
        emit_struct_set(body.ir, ptr, idx_val, val, &mut body.ctx);
    }

    // Constraint validation guards
    if let Some(field_defs) = body.ctx.struct_defs.get(name).cloned() {
        let layout = body.ctx.struct_layouts.get(name).cloned();
        for field_def in &field_defs {
            if field_def.constraints.is_empty() { continue; }
            let layout_idx = layout.as_ref().and_then(|l| l.field_index(&field_def.name));
            if fields.iter().any(|(n, _)| n == &field_def.name) && layout_idx.is_some() {
                let is_string = matches!(field_def.type_ref, roca::TypeRef::String);
                let field_idx = body.ir.const_i64(layout_idx.unwrap() as i64);
                let get_fn = if is_string { "__struct_get_ptr" } else { "__struct_get_f64" };
                if let Some(&get) = body.ctx.get_func(get_fn) {
                    let val = body.ir.call(get, &[ptr, field_idx]);
                    emit_value_constraints(body, val, is_string, &field_def.name, &field_def.constraints);
                }
            }
        }
    }

    ptr
}

/// Emit a constraint violation trap: if cond is non-zero, print error and return default.
fn emit_constraint_trap(body: &mut Body, cond: Value, field: &str, msg: &str) {
    let trap_block = body.ir.create_block();
    let ok_block = body.ir.create_block();
    body.ir.brif(cond, trap_block, ok_block);

    body.ir.switch_to(trap_block);
    body.ir.seal(trap_block);
    let err_msg = body.ir.leak_cstr(&format!("{}: {}", field, msg));
    if let Some(&panic_fn) = body.ctx.get_func("__constraint_panic") {
        body.ir.call_void(panic_fn, &[err_msg]);
    }
    let default = roca_cranelift::helpers::default_for_ir_type(body.ir.raw(), body.ctx.return_type);
    if body.ctx.returns_err {
        let err_tag = body.ir.raw().ins().iconst(types::I8, 1);
        body.ir.ret_with_err(default, err_tag);
    } else {
        body.ir.ret(default);
    }

    body.ir.switch_to(ok_block);
    body.ir.seal(ok_block);
}

/// Construct an enum variant as a tagged struct.
pub fn emit_enum_variant(body: &mut Body, variant: &str, args: &[Expr]) -> Value {
    let num_slots = 1 + args.len();
    let num_slots_val = body.ir.const_i64(num_slots as i64);

    let ptr = if let Some(&f) = body.ctx.get_func("__struct_alloc") {
        body.ir.call(f, &[num_slots_val])
    } else {
        return body.null();
    };

    let tag = body.ir.leak_cstr(variant);
    let tag_str = if let Some(&f) = body.ctx.get_func("__string_new") {
        body.ir.call(f, &[tag])
    } else { tag };
    let zero = body.null();
    emit_struct_set(body.ir, ptr, zero, tag_str, &mut body.ctx);

    for (i, arg) in args.iter().enumerate() {
        let val = emit_expr(body, arg);
        let idx = body.ir.const_i64((i + 1) as i64);
        emit_struct_set(body.ir, ptr, idx, val, &mut body.ctx);
    }
    ptr
}

/// Emit a crash-handled call with retry support.
/// If the crash chain includes Retry, wraps the call in a loop.
/// `func_ref` and `arg_slots` allow re-calling the function on retry.
pub fn emit_crash_call(
    body: &mut Body,
    func_ref: FuncRef,
    arg_slots: &[VarSlot],
    arg_types: &[ir::Type],
    handler: &CrashHandlerKind,
) -> Value {
    let chain = match handler {
        CrashHandlerKind::Simple(chain) => chain.clone(),
        CrashHandlerKind::Detailed { default, .. } => {
            default.clone().unwrap_or_else(|| vec![CrashStep::Halt])
        }
    };

    // Check if chain has retry
    let retry = chain.iter().find_map(|s| {
        if let CrashStep::Retry { attempts, delay_ms } = s { Some((*attempts, *delay_ms)) } else { None }
    });

    // Make the initial call
    let args: Vec<Value> = arg_slots.iter().zip(arg_types).map(|(s, t)| {
        body.ir.raw().ins().stack_load(*t, s.0, 0)
    }).collect();
    let results = body.ir.call_multi(func_ref, &args);
    if results.len() < 2 {
        return if results.is_empty() { body.null() } else { results[0] };
    }

    let result_type = body.ir.value_ir_type(results[0]);
    let value_slot = body.ir.alloc_var(results[0]);
    let err_slot = body.ir.alloc_var(results[1]);

    if let Some((attempts, delay_ms)) = retry {
        // Retry loop: header checks counter, body re-calls and checks result
        let header = body.ir.create_block();
        let retry_body = body.ir.create_block();
        let done = body.ir.create_block();

        // Init counter, check first result
        let counter_init = body.ir.const_i64(1); // attempt 0 already done
        let counter_slot = body.ir.alloc_var(counter_init);
        let first_err = body.ir.raw().ins().stack_load(types::I8, err_slot.0, 0);
        body.ir.brif(first_err, header, done);

        // Header: check if more attempts remain
        body.ir.switch_to(header);
        let counter = body.ir.raw().ins().stack_load(types::I64, counter_slot.0, 0);
        let max = body.ir.const_i64(attempts as i64);
        let has_more = body.ir.raw().ins().icmp(ir::condcodes::IntCC::SignedLessThan, counter, max);
        body.ir.brif(has_more, retry_body, done);

        // Body: sleep, re-call, increment counter
        body.ir.switch_to(retry_body);
        body.ir.seal(retry_body);

        if delay_ms > 0 {
            if let Some(&sleep_fn) = body.ctx.get_func("__sleep") {
                let ms = body.ir.const_number(delay_ms as f64);
                body.ir.call_void(sleep_fn, &[ms]);
            }
        }

        let retry_args: Vec<Value> = arg_slots.iter().zip(arg_types).map(|(s, t)| {
            body.ir.raw().ins().stack_load(*t, s.0, 0)
        }).collect();
        let retry_results = body.ir.call_multi(func_ref, &retry_args);
        body.ir.raw().ins().stack_store(retry_results[0], value_slot.0, 0);
        body.ir.raw().ins().stack_store(retry_results[1], err_slot.0, 0);

        // Increment counter
        let cur = body.ir.raw().ins().stack_load(types::I64, counter_slot.0, 0);
        let one = body.ir.const_i64(1);
        let next = body.ir.iadd(cur, one);
        body.ir.raw().ins().stack_store(next, counter_slot.0, 0);

        // Check result — success breaks, failure loops back to header
        let retry_err = body.ir.raw().ins().stack_load(types::I8, err_slot.0, 0);
        body.ir.brif(retry_err, header, done);

        body.ir.seal(header); // sealed after back-edge from body

        body.ir.switch_to(done);
        body.ir.seal(done);
    }

    // After retry (or no retry): check final error state
    let final_value = body.ir.raw().ins().stack_load(result_type, value_slot.0, 0);
    let final_err = body.ir.raw().ins().stack_load(types::I8, err_slot.0, 0);
    emit_crash_handler(body, final_value, final_err, handler)
}

pub fn emit_crash_handler(
    body: &mut Body,
    value: Value,
    err_tag: Value,
    handler: &CrashHandlerKind,
) -> Value {
    let ok_block = body.ir.create_block();
    let err_block = body.ir.create_block();
    let merge = body.ir.create_block();
    let result_type = body.ir.value_ir_type(value);
    body.ir.append_block_param(merge, &roca_type_for_ir(result_type));

    body.ir.brif(err_tag, err_block, ok_block);

    body.ir.switch_to(ok_block);
    body.ir.seal(ok_block);
    body.ir.jump_with(merge, value);

    body.ir.switch_to(err_block);
    body.ir.seal(err_block);

    let chain = match handler {
        CrashHandlerKind::Simple(chain) => chain.clone(),
        CrashHandlerKind::Detailed { default, .. } => {
            default.clone().unwrap_or_else(|| vec![CrashStep::Halt])
        }
    };

    // Filter out Retry — already handled by emit_crash_call
    let chain: Vec<_> = chain.into_iter().filter(|s| !matches!(s, CrashStep::Retry { .. })).collect();

    let terminates = chain.iter().any(|s| matches!(s, CrashStep::Halt | CrashStep::Panic));
    let err_result = emit_crash_chain(body, &chain, result_type);

    if !terminates {
        body.ir.jump_with(merge, err_result);
    }

    body.ir.switch_to(merge);
    body.ir.seal(merge);
    body.ir.block_param(merge, 0)
}

fn emit_crash_chain(
    body: &mut Body,
    chain: &[CrashStep],
    result_type: ir::Type,
) -> Value {
    let mut last_value = roca_cranelift::helpers::default_for_ir_type(body.ir.raw(), result_type);

    for step in chain {
        match step {
            CrashStep::Log => {
                let msg_val = body.ir.leak_cstr("error");
                if let Some(&f) = body.ctx.get_func("__print") {
                    body.ir.call_void(f, &[msg_val]);
                }
            }
            CrashStep::Halt => {
                emit_scope_cleanup(body.ir, &body.ctx, None);
                if body.ctx.returns_err {
                    let err = body.ir.raw().ins().iconst(types::I8, 1);
                    body.ir.ret_with_err(last_value, err);
                } else {
                    body.ir.ret(last_value);
                }
                return last_value;
            }
            CrashStep::Panic => {
                body.ir.trap(1);
                return last_value;
            }
            CrashStep::Skip => {}
            CrashStep::Fallback(expr) => {
                last_value = emit_expr(body, expr);
            }
            CrashStep::Retry { attempts, delay_ms: _ } => {
                let _ = attempts;
            }
        }
    }
    last_value
}

/// Inline map/filter: emit a loop that applies the closure body to each element.
fn emit_inline_map_filter(
    body: &mut Body,
    target: &Expr,
    method: &str,
    params: &[String],
    closure_body: &Expr,
) -> Value {
    let arr = emit_expr(body, target);
    let is_filter = method == "filter";

    let result_arr = if let Some(&f) = body.ctx.get_func("__array_new") {
        body.ir.call(f, &[])
    } else { return body.null(); };

    let len = if let Some(&f) = body.ctx.get_func("__array_len") {
        body.ir.call(f, &[arr])
    } else { return result_arr; };
    let len_slot = body.ir.alloc_var(len);
    let arr_slot = body.ir.alloc_var(arr);
    let result_slot = body.ir.alloc_var(result_arr);

    let zero = body.null();
    let idx_slot = body.ir.alloc_var(zero);
    let header = body.ir.create_block();
    let body_block = body.ir.create_block();
    let exit = body.ir.create_block();

    body.ir.jump(header);
    body.ir.switch_to(header);
    let idx = body.ir.raw().ins().stack_load(types::I64, idx_slot.0, 0);
    let len_val = body.ir.raw().ins().stack_load(types::I64, len_slot.0, 0);
    let cond = body.ir.raw().ins().icmp(ir::condcodes::IntCC::SignedLessThan, idx, len_val);
    body.ir.brif(cond, body_block, exit);

    body.ir.switch_to(body_block);
    body.ir.seal(body_block);

    let cur_idx = body.ir.raw().ins().stack_load(types::I64, idx_slot.0, 0);
    let cur_arr = body.ir.raw().ins().stack_load(types::I64, arr_slot.0, 0);
    let elem = if let Some(&f) = body.ctx.get_func("__array_get_f64") {
        body.ir.call(f, &[cur_arr, cur_idx])
    } else { body.number(0.0) };

    let param_name = params.first().cloned().unwrap_or_default();
    let elem_slot = body.ir.alloc_var(elem);
    body.ctx.set_var_kind(param_name, elem_slot.0, types::F64, RocaType::Number);

    let result = emit_expr(body, closure_body);

    if is_filter {
        let then_push = body.ir.create_block();
        let after_push = body.ir.create_block();
        body.ir.brif(result, then_push, after_push);

        body.ir.switch_to(then_push);
        body.ir.seal(then_push);
        let push_elem = body.ir.raw().ins().stack_load(types::F64, elem_slot.0, 0);
        let res_arr = body.ir.raw().ins().stack_load(types::I64, result_slot.0, 0);
        if let Some(&f) = body.ctx.get_func("__array_push_f64") {
            body.ir.call_void(f, &[res_arr, push_elem]);
        }
        body.ir.jump(after_push);

        body.ir.switch_to(after_push);
        body.ir.seal(after_push);
    } else {
        let res_arr = body.ir.raw().ins().stack_load(types::I64, result_slot.0, 0);
        emit_array_push(body.ir, res_arr, result, &mut body.ctx);
    }

    let next_idx = body.ir.raw().ins().stack_load(types::I64, idx_slot.0, 0);
    let one = body.ir.const_i64(1);
    let incremented = body.ir.iadd(next_idx, one);
    body.ir.raw().ins().stack_store(incremented, idx_slot.0, 0);
    body.ir.jump(header);
    body.ir.seal(header);

    body.ir.switch_to(exit);
    body.ir.seal(exit);

    body.ir.raw().ins().stack_load(types::I64, result_slot.0, 0)
}
