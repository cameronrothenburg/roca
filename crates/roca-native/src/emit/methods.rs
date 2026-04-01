//! Method calls, struct/enum construction, crash handlers, and inline map/filter.

use cranelift_codegen::ir::{self, types, Value, BlockArg, InstBuilder};
use cranelift_frontend::FunctionBuilder;

use roca_ast::{self as roca, Expr, crash::{CrashHandlerKind, CrashStep}};
use crate::helpers::{
    call_rt, call_void, alloc_slot, load_slot, ensure_i64, leak_cstr, default_for_ir_type,
};
use super::context::{EmitCtx, StructLayout, ValKind};
use super::helpers::{
    infer_kind, emit_scope_cleanup, target_kind, first_arg_or_null,
    emit_array_push, emit_struct_set, emit_length,
};
use super::expr::emit_expr;

/// Emit constraint validation guards for function parameters at entry.
pub fn emit_param_constraints(b: &mut FunctionBuilder, params: &[roca::Param], ctx: &mut EmitCtx) {
    for param in params {
        if param.constraints.is_empty() { continue; }
        let var = match ctx.get_var(&param.name) {
            Some(v) => v.clone(),
            None => continue,
        };
        let val = load_slot(b, var.slot, var.cranelift_type);
        let is_string = matches!(param.type_ref, roca::TypeRef::String);
        emit_value_constraints(b, val, is_string, &param.name, &param.constraints, ctx);
    }
}

/// Shared constraint guard emission for a pre-loaded value.
/// Used by both param constraints and struct field constraints.
pub fn emit_value_constraints(
    b: &mut FunctionBuilder,
    val: Value,
    is_string: bool,
    name: &str,
    constraints: &[roca::Constraint],
    ctx: &EmitCtx,
) {
    for constraint in constraints {
        match constraint {
            roca::Constraint::Min(n) if !is_string => {
                let min_val = b.ins().f64const(*n);
                let cmp = b.ins().fcmp(ir::condcodes::FloatCC::LessThan, val, min_val);
                let cmp_ext = b.ins().uextend(types::I64, cmp);
                emit_constraint_trap(b, cmp_ext, name, &format!("must be >= {}", n), ctx);
            }
            roca::Constraint::Max(n) if !is_string => {
                let max_val = b.ins().f64const(*n);
                let cmp = b.ins().fcmp(ir::condcodes::FloatCC::GreaterThan, val, max_val);
                let cmp_ext = b.ins().uextend(types::I64, cmp);
                emit_constraint_trap(b, cmp_ext, name, &format!("must be <= {}", n), ctx);
            }
            roca::Constraint::Min(n) | roca::Constraint::MinLen(n) if is_string => {
                if let Some(&len_fn) = ctx.get_func("__string_len") {
                    let len = call_rt(b, len_fn, &[val]);
                    let min_val = b.ins().iconst(types::I64, *n as i64);
                    let cmp = b.ins().icmp(ir::condcodes::IntCC::SignedLessThan, len, min_val);
                    let cmp_ext = b.ins().uextend(types::I64, cmp);
                    emit_constraint_trap(b, cmp_ext, name, &format!("min length {}", n), ctx);
                }
            }
            roca::Constraint::Max(n) | roca::Constraint::MaxLen(n) if is_string => {
                if let Some(&len_fn) = ctx.get_func("__string_len") {
                    let len = call_rt(b, len_fn, &[val]);
                    let max_val = b.ins().iconst(types::I64, *n as i64);
                    let cmp = b.ins().icmp(ir::condcodes::IntCC::SignedGreaterThan, len, max_val);
                    let cmp_ext = b.ins().uextend(types::I64, cmp);
                    emit_constraint_trap(b, cmp_ext, name, &format!("max length {}", n), ctx);
                }
            }
            roca::Constraint::Contains(s) => {
                let needle = leak_cstr(b, s);
                if let Some(&includes) = ctx.get_func("__string_includes") {
                    let result = call_rt(b, includes, &[val, needle]);
                    let not_result = {
                        let ext = b.ins().uextend(types::I64, result);
                        let one = b.ins().iconst(types::I64, 1);
                        b.ins().isub(one, ext)
                    };
                    emit_constraint_trap(b, not_result, name, &format!("must contain \"{}\"", s), ctx);
                }
            }
            _ => {}
        }
    }
}

pub fn emit_method_call(b: &mut FunctionBuilder, target: &Expr, method: &str, args: &[Expr], ctx: &mut EmitCtx) -> Value {
    // Enum data variant constructor: Token.Number(42)
    if let Expr::Ident(name) = target {
        if ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&method.to_string())) {
            return emit_enum_variant(b, method, args, ctx);
        }
    }

    // Struct static method / extern contract method call: Counter.current(c), Fs.readFile(path)
    if let Expr::Ident(type_name) = target {
        let qualified = format!("{}.{}", type_name, method);
        if let Some(&func_ref) = ctx.get_func(&qualified) {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(b, a, ctx)).collect();

            if let Some(handler) = ctx.crash_handlers.get(&qualified).cloned() {
                // Store args in slots for potential retry
                let arg_slots: Vec<_> = arg_vals.iter().map(|v| alloc_slot(b, *v)).collect();
                let arg_types: Vec<_> = arg_vals.iter().map(|v| b.func.dfg.value_type(*v)).collect();
                return emit_crash_call(b, func_ref, &arg_slots, &arg_types, &handler, ctx);
            }

            let call = b.ins().call(func_ref, &arg_vals);
            let results = b.inst_results(call).to_vec();
            if results.len() >= 2 { return results[0]; }
            if !results.is_empty() { return results[0]; }
        }
    }

    let kind = target_kind(target, ctx);

    // Inline map/filter before evaluating target — they need the closure
    if (method == "map" || method == "filter") && !args.is_empty() {
        if let Expr::Closure { params, body } = &args[0] {
            return emit_inline_map_filter(b, target, method, params, body, ctx);
        }
    }

    // Detect chained method calls that produce intermediate strings
    let target_is_temp_string = !matches!(target, Expr::Ident(_) | Expr::String(_))
        && infer_kind(target, ctx) == ValKind::String;

    let obj = emit_expr(b, target, ctx);

    let result = match method {
        "push" => {
            if let Some(arg) = args.first() {
                let val = emit_expr(b, arg, ctx);
                emit_array_push(b, obj, val, ctx);
            }
            b.ins().iconst(types::I8, 0)
        }
        "pop" => {
            if let Some(&get) = ctx.get_func("__array_get_f64") {
                if let Some(&len_fn) = ctx.get_func("__array_len") {
                    let len = call_rt(b, len_fn, &[obj]);
                    let one = b.ins().iconst(types::I64, 1);
                    let last_idx = b.ins().isub(len, one);
                    return call_rt(b, get, &[obj, last_idx]);
                }
            }
            b.ins().f64const(0.0)
        }
        "join" => {
            let sep = if let Some(arg) = args.first() {
                emit_expr(b, arg, ctx)
            } else {
                leak_cstr(b, ",")
            };
            if let Some(&f) = ctx.get_func("__array_join") {
                call_rt(b, f, &[obj, sep])
            } else { b.ins().iconst(types::I64, 0) }
        }

        "includes" | "contains" => {
            let needle = first_arg_or_null(b, args, ctx);
            if let Some(&f) = ctx.get_func("__string_includes") {
                let result = call_rt(b, f, &[obj, needle]);
                b.ins().uextend(types::I64, result)
            } else { b.ins().iconst(types::I64, 0) }
        }
        "startsWith" => {
            let prefix = first_arg_or_null(b, args, ctx);
            if let Some(&f) = ctx.get_func("__string_starts_with") {
                let result = call_rt(b, f, &[obj, prefix]);
                b.ins().uextend(types::I64, result)
            } else { b.ins().iconst(types::I64, 0) }
        }
        "endsWith" => {
            let suffix = first_arg_or_null(b, args, ctx);
            if let Some(&f) = ctx.get_func("__string_ends_with") {
                let result = call_rt(b, f, &[obj, suffix]);
                b.ins().uextend(types::I64, result)
            } else { b.ins().iconst(types::I64, 0) }
        }
        "trim" => {
            if let Some(&f) = ctx.get_func("__string_trim") {
                call_rt(b, f, &[obj])
            } else { obj }
        }
        "toUpperCase" => {
            if let Some(&f) = ctx.get_func("__string_to_upper") {
                call_rt(b, f, &[obj])
            } else { obj }
        }
        "toLowerCase" => {
            if let Some(&f) = ctx.get_func("__string_to_lower") {
                call_rt(b, f, &[obj])
            } else { obj }
        }
        "slice" => {
            let start = args.first().map(|a| emit_expr(b, a, ctx))
                .unwrap_or_else(|| b.ins().iconst(types::I64, 0));
            let end = args.get(1).map(|a| emit_expr(b, a, ctx))
                .unwrap_or_else(|| {
                    if let Some(&f) = ctx.get_func("__string_len") {
                        call_rt(b, f, &[obj])
                    } else { b.ins().iconst(types::I64, 0) }
                });
            let start_i = ensure_i64(b, start);
            let end_i = ensure_i64(b, end);
            if let Some(&f) = ctx.get_func("__string_slice") {
                call_rt(b, f, &[obj, start_i, end_i])
            } else { obj }
        }
        "split" => {
            let delim = first_arg_or_null(b, args, ctx);
            if let Some(&f) = ctx.get_func("__string_split") {
                call_rt(b, f, &[obj, delim])
            } else { b.ins().iconst(types::I64, 0) }
        }
        "charAt" => {
            let idx = first_arg_or_null(b, args, ctx);
            let idx_i = ensure_i64(b, idx);
            if let Some(&f) = ctx.get_func("__string_char_at") {
                call_rt(b, f, &[obj, idx_i])
            } else { b.ins().iconst(types::I64, 0) }
        }
        "charCodeAt" => {
            let idx = first_arg_or_null(b, args, ctx);
            let idx_i = ensure_i64(b, idx);
            if let Some(&f) = ctx.get_func("__string_char_code_at") {
                call_rt(b, f, &[obj, idx_i])
            } else { b.ins().f64const(0.0) }
        }
        "indexOf" => {
            let needle = first_arg_or_null(b, args, ctx);
            if let Some(&f) = ctx.get_func("__string_index_of") {
                call_rt(b, f, &[obj, needle])
            } else { b.ins().f64const(-1.0) }
        }

        "len" | "length" => emit_length(b, obj, kind, ctx),
        "toString" => {
            let ty = b.func.dfg.value_type(obj);
            if ty == types::F64 {
                if let Some(&f) = ctx.get_func("__string_from_f64") {
                    call_rt(b, f, &[obj])
                } else { b.ins().iconst(types::I64, 0) }
            } else {
                obj
            }
        }
        _ => obj,
    };

    // Free intermediate string from chained method calls
    if target_is_temp_string {
        if let Some(&f) = ctx.get_func("__rc_release") {
            call_void(b, f, &[obj]);
        }
    }

    result
}

pub fn emit_struct_lit(b: &mut FunctionBuilder, name: &str, fields: &[(String, Expr)], ctx: &mut EmitCtx) -> Value {
    if !ctx.struct_layouts.contains_key(name) {
        ctx.struct_layouts.insert(name.to_string(), StructLayout {
            fields: fields.iter().map(|(n, v)| (n.clone(), infer_kind(v, ctx))).collect(),
        });
    }

    let num_fields = b.ins().iconst(types::I64, fields.len() as i64);
    let ptr = if let Some(f) = ctx.get_func("__struct_alloc") {
        call_rt(b, *f, &[num_fields])
    } else {
        return b.ins().iconst(types::I64, 0);
    };

    let indices: Vec<usize> = {
        let layout = ctx.struct_layouts.get(name).unwrap();
        fields.iter().map(|(n, _)| layout.field_index(n).unwrap_or(0)).collect()
    };

    for (i, (_, field_expr)) in fields.iter().enumerate() {
        let val = emit_expr(b, field_expr, ctx);
        let idx_val = b.ins().iconst(types::I64, indices[i] as i64);
        emit_struct_set(b, ptr, idx_val, val, ctx);
    }

    // Constraint validation guards
    if let Some(field_defs) = ctx.struct_defs.get(name).cloned() {
        let layout = ctx.struct_layouts.get(name).cloned();
        for field_def in &field_defs {
            if field_def.constraints.is_empty() { continue; }
            let layout_idx = layout.as_ref().and_then(|l| l.field_index(&field_def.name));
            if fields.iter().any(|(n, _)| n == &field_def.name) && layout_idx.is_some() {
                let is_string = matches!(field_def.type_ref, roca::TypeRef::String);
                let field_idx = b.ins().iconst(types::I64, layout_idx.unwrap() as i64);
                let get_fn = if is_string { "__struct_get_ptr" } else { "__struct_get_f64" };
                if let Some(&get) = ctx.get_func(get_fn) {
                    let val = call_rt(b, get, &[ptr, field_idx]);
                    emit_value_constraints(b, val, is_string, &field_def.name, &field_def.constraints, ctx);
                }
            }
        }
    }

    ptr
}

/// Emit a constraint violation trap: if cond is non-zero, print error and return default.
fn emit_constraint_trap(b: &mut FunctionBuilder, cond: Value, field: &str, msg: &str, ctx: &EmitCtx) {
    let trap_block = b.create_block();
    let ok_block = b.create_block();
    b.ins().brif(cond, trap_block, &[], ok_block, &[]);

    b.switch_to_block(trap_block);
    b.seal_block(trap_block);
    let err_msg = leak_cstr(b, &format!("{}: {}", field, msg));
    if let Some(&panic_fn) = ctx.get_func("__constraint_panic") {
        call_void(b, panic_fn, &[err_msg]);
    }
    let default = default_for_ir_type(b, ctx.return_type);
    if ctx.returns_err {
        let err_tag = b.ins().iconst(types::I8, 1);
        b.ins().return_(&[default, err_tag]);
    } else {
        b.ins().return_(&[default]);
    }

    b.switch_to_block(ok_block);
    b.seal_block(ok_block);
}

/// Construct an enum variant as a tagged struct.
pub fn emit_enum_variant(b: &mut FunctionBuilder, variant: &str, args: &[Expr], ctx: &mut EmitCtx) -> Value {
    let num_slots = 1 + args.len();
    let num_slots_val = b.ins().iconst(types::I64, num_slots as i64);

    let ptr = if let Some(&f) = ctx.get_func("__struct_alloc") {
        call_rt(b, f, &[num_slots_val])
    } else {
        return b.ins().iconst(types::I64, 0);
    };

    let tag = leak_cstr(b, variant);
    let tag_str = if let Some(&f) = ctx.get_func("__string_new") {
        call_rt(b, f, &[tag])
    } else { tag };
    let zero = b.ins().iconst(types::I64, 0);
    emit_struct_set(b, ptr, zero, tag_str, ctx);

    for (i, arg) in args.iter().enumerate() {
        let val = emit_expr(b, arg, ctx);
        let idx = b.ins().iconst(types::I64, (i + 1) as i64);
        emit_struct_set(b, ptr, idx, val, ctx);
    }
    ptr
}

/// Emit a crash-handled call with retry support.
/// If the crash chain includes Retry, wraps the call in a loop.
/// `func_ref` and `arg_slots` allow re-calling the function on retry.
pub fn emit_crash_call(
    b: &mut FunctionBuilder,
    func_ref: ir::FuncRef,
    arg_slots: &[ir::StackSlot],
    arg_types: &[ir::Type],
    handler: &CrashHandlerKind,
    ctx: &mut EmitCtx,
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
    let args: Vec<Value> = arg_slots.iter().zip(arg_types).map(|(s, t)| load_slot(b, *s, *t)).collect();
    let call = b.ins().call(func_ref, &args);
    let results = b.inst_results(call).to_vec();
    if results.len() < 2 {
        return if results.is_empty() { b.ins().iconst(types::I64, 0) } else { results[0] };
    }

    let result_type = b.func.dfg.value_type(results[0]);
    let value_slot = alloc_slot(b, results[0]);
    let err_slot = alloc_slot(b, results[1]);

    if let Some((attempts, delay_ms)) = retry {
        // Retry loop: header checks counter, body re-calls and checks result
        let header = b.create_block();
        let body = b.create_block();
        let done = b.create_block();

        // Init counter, check first result
        let counter_init = b.ins().iconst(types::I64, 1); // attempt 0 already done
        let counter_slot = alloc_slot(b, counter_init);
        let first_err = load_slot(b, err_slot, types::I8);
        b.ins().brif(first_err, header, &[], done, &[]);

        // Header: check if more attempts remain
        b.switch_to_block(header);
        let counter = load_slot(b, counter_slot, types::I64);
        let max = b.ins().iconst(types::I64, attempts as i64);
        let has_more = b.ins().icmp(ir::condcodes::IntCC::SignedLessThan, counter, max);
        b.ins().brif(has_more, body, &[], done, &[]);

        // Body: sleep, re-call, increment counter
        b.switch_to_block(body);
        b.seal_block(body);

        if delay_ms > 0 {
            if let Some(&sleep_fn) = ctx.get_func("__sleep") {
                let ms = b.ins().f64const(delay_ms as f64);
                call_void(b, sleep_fn, &[ms]);
            }
        }

        let retry_args: Vec<Value> = arg_slots.iter().zip(arg_types).map(|(s, t)| load_slot(b, *s, *t)).collect();
        let retry_call = b.ins().call(func_ref, &retry_args);
        let retry_results = b.inst_results(retry_call).to_vec();
        b.ins().stack_store(retry_results[0], value_slot, 0);
        b.ins().stack_store(retry_results[1], err_slot, 0);

        // Increment counter
        let cur = load_slot(b, counter_slot, types::I64);
        let one = b.ins().iconst(types::I64, 1);
        let next = b.ins().iadd(cur, one);
        b.ins().stack_store(next, counter_slot, 0);

        // Check result — success breaks, failure loops back to header
        let retry_err = load_slot(b, err_slot, types::I8);
        b.ins().brif(retry_err, header, &[], done, &[]);

        b.seal_block(header); // sealed after back-edge from body

        b.switch_to_block(done);
        b.seal_block(done);
    }

    // After retry (or no retry): check final error state
    let final_value = load_slot(b, value_slot, result_type);
    let final_err = load_slot(b, err_slot, types::I8);
    emit_crash_handler(b, final_value, final_err, handler, ctx)
}

pub fn emit_crash_handler(
    b: &mut FunctionBuilder,
    value: Value,
    err_tag: Value,
    handler: &CrashHandlerKind,
    ctx: &mut EmitCtx,
) -> Value {
    let ok_block = b.create_block();
    let err_block = b.create_block();
    let merge = b.create_block();
    let result_type = b.func.dfg.value_type(value);
    b.append_block_param(merge, result_type);

    b.ins().brif(err_tag, err_block, &[], ok_block, &[]);

    b.switch_to_block(ok_block);
    b.seal_block(ok_block);
    b.ins().jump(merge, &[BlockArg::Value(value)]);

    b.switch_to_block(err_block);
    b.seal_block(err_block);

    let chain = match handler {
        CrashHandlerKind::Simple(chain) => chain.clone(),
        CrashHandlerKind::Detailed { default, .. } => {
            default.clone().unwrap_or_else(|| vec![CrashStep::Halt])
        }
    };

    // Filter out Retry — already handled by emit_crash_call
    let chain: Vec<_> = chain.into_iter().filter(|s| !matches!(s, CrashStep::Retry { .. })).collect();

    let terminates = chain.iter().any(|s| matches!(s, CrashStep::Halt | CrashStep::Panic));
    let err_result = emit_crash_chain(b, &chain, result_type, ctx);

    if !terminates {
        b.ins().jump(merge, &[BlockArg::Value(err_result)]);
    }

    b.switch_to_block(merge);
    b.seal_block(merge);
    b.block_params(merge)[0]
}

fn emit_crash_chain(
    b: &mut FunctionBuilder,
    chain: &[CrashStep],
    result_type: ir::Type,
    ctx: &mut EmitCtx,
) -> Value {
    let mut last_value = default_for_ir_type(b, result_type);

    for step in chain {
        match step {
            CrashStep::Log => {
                let msg_val = leak_cstr(b, "error");
                if let Some(&f) = ctx.get_func("__print") {
                    call_void(b, f, &[msg_val]);
                }
            }
            CrashStep::Halt => {
                emit_scope_cleanup(b, ctx, None);
                if ctx.returns_err {
                    let err = b.ins().iconst(types::I8, 1);
                    b.ins().return_(&[last_value, err]);
                } else {
                    b.ins().return_(&[last_value]);
                }
                return last_value;
            }
            CrashStep::Panic => {
                b.ins().trap(ir::TrapCode::unwrap_user(1));
                return last_value;
            }
            CrashStep::Skip => {}
            CrashStep::Fallback(expr) => {
                last_value = emit_expr(b, expr, ctx);
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
    b: &mut FunctionBuilder,
    target: &Expr,
    method: &str,
    params: &[String],
    body: &Expr,
    ctx: &mut EmitCtx,
) -> Value {
    let arr = emit_expr(b, target, ctx);
    let is_filter = method == "filter";

    let result_arr = if let Some(&f) = ctx.get_func("__array_new") {
        call_rt(b, f, &[])
    } else { return b.ins().iconst(types::I64, 0); };

    let len = if let Some(&f) = ctx.get_func("__array_len") {
        call_rt(b, f, &[arr])
    } else { return result_arr; };
    let len_slot = alloc_slot(b, len);
    let arr_slot = alloc_slot(b, arr);
    let result_slot = alloc_slot(b, result_arr);

    let zero = b.ins().iconst(types::I64, 0);
    let idx_slot = alloc_slot(b, zero);
    let header = b.create_block();
    let body_block = b.create_block();
    let exit = b.create_block();

    b.ins().jump(header, &[]);
    b.switch_to_block(header);
    let idx = load_slot(b, idx_slot, types::I64);
    let len_val = load_slot(b, len_slot, types::I64);
    let cond = b.ins().icmp(ir::condcodes::IntCC::SignedLessThan, idx, len_val);
    b.ins().brif(cond, body_block, &[], exit, &[]);

    b.switch_to_block(body_block);
    b.seal_block(body_block);

    let cur_idx = load_slot(b, idx_slot, types::I64);
    let cur_arr = load_slot(b, arr_slot, types::I64);
    let elem = if let Some(&f) = ctx.get_func("__array_get_f64") {
        call_rt(b, f, &[cur_arr, cur_idx])
    } else { b.ins().f64const(0.0) };

    let param_name = params.first().cloned().unwrap_or_default();
    let elem_slot = alloc_slot(b, elem);
    ctx.set_var_kind(param_name, elem_slot, types::F64, ValKind::Number);

    let result = emit_expr(b, body, ctx);

    if is_filter {
        let then_push = b.create_block();
        let after_push = b.create_block();
        b.ins().brif(result, then_push, &[], after_push, &[]);

        b.switch_to_block(then_push);
        b.seal_block(then_push);
        let push_elem = load_slot(b, elem_slot, types::F64);
        let res_arr = load_slot(b, result_slot, types::I64);
        if let Some(&f) = ctx.get_func("__array_push_f64") {
            call_void(b, f, &[res_arr, push_elem]);
        }
        b.ins().jump(after_push, &[]);

        b.switch_to_block(after_push);
        b.seal_block(after_push);
    } else {
        let res_arr = load_slot(b, result_slot, types::I64);
        emit_array_push(b, res_arr, result, ctx);
    }

    let next_idx = load_slot(b, idx_slot, types::I64);
    let one = b.ins().iconst(types::I64, 1);
    let incremented = b.ins().iadd(next_idx, one);
    b.ins().stack_store(incremented, idx_slot, 0);
    b.ins().jump(header, &[]);
    b.seal_block(header);

    b.switch_to_block(exit);
    b.seal_block(exit);

    load_slot(b, result_slot, types::I64)
}
