//! Expression emission — Roca expressions to Cranelift IR values.

use cranelift_codegen::ir::InstBuilder;
use roca_ast::{self as roca, Expr, BinOp, StringPart};
use roca_types::RocaType;
use roca_cranelift::api::{Body, Value};
use roca_cranelift::context::ValKind;
use roca_cranelift::cranelift_type::CraneliftType;
use super::helpers::{infer_kind, emit_length, target_kind, emit_array_push};

pub fn emit_expr(body: &mut Body, expr: &Expr) -> Value {
    match expr {
        Expr::Number(n) => body.number(*n),
        Expr::Bool(v) => body.bool_val(*v),
        Expr::String(s) => body.string(s),
        Expr::Ident(name) => {
            if let Some(var) = body.ctx.get_var(name) {
                body.ir.raw().ins().stack_load(var.cranelift_type, var.slot, 0)
            } else {
                body.null()
            }
        }
        Expr::BinOp { left, op, right } => {
            let l_is_temp_string = matches!(op, BinOp::Add)
                && !matches!(left.as_ref(), Expr::Ident(_) | Expr::Number(_) | Expr::Bool(_) | Expr::Null)
                && infer_kind(left, &body.ctx) == RocaType::String;
            let l = emit_expr(body, left);
            let r = emit_expr(body, right);
            let result = emit_binop(body, op, l, r);
            if l_is_temp_string {
                if let Some(&f) = body.ctx.get_func("__rc_release") {
                    body.ir.call_void(f, &[l]);
                }
            }
            result
        }
        Expr::StructLit { name, fields } => super::methods::emit_struct_lit(body, name, fields),
        Expr::Call { target, args } => emit_call(body, target, args),
        Expr::Array(elements) => emit_array_literal(body, elements),
        Expr::Index { target, index } => emit_index(body, target, index),
        Expr::Not(inner) => {
            let val = emit_expr(body, inner);
            body.not(val)
        }
        Expr::Closure { params, body: closure_body } => emit_closure(body, params, closure_body),
        Expr::SelfRef => body.self_ref(),
        Expr::Null => body.null(),
        Expr::StringInterp(parts) => emit_string_interp(body, parts),
        Expr::Match { value, arms } => emit_match(body, value, arms),
        Expr::FieldAccess { target, field } => emit_field_access(body, target, field),
        Expr::EnumVariant { enum_name: _, variant, args } => {
            super::methods::emit_enum_variant(body, variant, args)
        }
        Expr::Await(inner) => emit_expr(body, inner),
    }
}

fn emit_binop(body: &mut Body, op: &BinOp, l: Value, r: Value) -> Value {
    let is_float = body.ir.is_number(l);

    match op {
        BinOp::Add if is_float => body.ir.add(l, r),
        BinOp::Add => {
            if let Some(f) = body.ctx.get_func("__string_concat") { body.ir.call(*f, &[l, r]) }
            else { body.ir.iadd(l, r) }
        }
        BinOp::Sub => body.ir.sub(l, r),
        BinOp::Mul => body.ir.mul(l, r),
        BinOp::Div => body.ir.div(l, r),
        BinOp::Eq if is_float => body.ir.f_eq(l, r),
        BinOp::Eq => {
            if let Some(f) = body.ctx.get_func("__string_eq") {
                let result = body.ir.call(*f, &[l, r]);
                body.ir.extend_bool(result)
            } else {
                body.ir.i_eq(l, r)
            }
        }
        BinOp::Neq if is_float => body.ir.f_ne(l, r),
        BinOp::Neq => {
            if let Some(f) = body.ctx.get_func("__string_eq") {
                let eq = body.ir.call(*f, &[l, r]);
                let ext = body.ir.extend_bool(eq);
                let one = body.ir.const_i64(1);
                body.ir.isub(one, ext)
            } else {
                body.ir.i_ne(l, r)
            }
        }
        BinOp::Lt => body.ir.f_lt(l, r),
        BinOp::Gt => body.ir.f_gt(l, r),
        BinOp::Lte => body.ir.f_le(l, r),
        BinOp::Gte => body.ir.f_ge(l, r),
        BinOp::And => body.ir.bool_and(l, r),
        BinOp::Or => body.ir.bool_or(l, r),
    }
}

fn emit_string_interp(body: &mut Body, parts: &[StringPart]) -> Value {
    let concat = body.ctx.get_func("__string_concat").copied();
    let to_str = body.ctx.get_func("__string_from_f64").copied();
    let string_new = body.ctx.get_func("__string_new").copied();

    let mut result: Option<Value> = None;
    for part in parts {
        let val = match part {
            StringPart::Literal(s) => {
                let static_ptr = body.ir.leak_cstr(s);
                if let Some(f) = string_new { body.ir.call(f, &[static_ptr]) } else { static_ptr }
            }
            StringPart::Expr(expr) => {
                let v = emit_expr(body, expr);
                if body.ir.is_number(v) {
                    if let Some(f) = to_str { body.ir.call(f, &[v]) } else { v }
                } else {
                    v
                }
            }
        };
        result = Some(match result {
            None => val,
            Some(acc) => {
                if let Some(f) = concat { body.ir.call(f, &[acc, val]) } else { val }
            }
        });
    }
    result.unwrap_or_else(|| body.null())
}

fn emit_match(body: &mut Body, value: &Expr, arms: &[roca::MatchArm]) -> Value {
    let scrutinee = emit_expr(body, value);
    let is_float = body.ir.is_number(scrutinee);

    let default_arm = arms.iter().find(|a| a.pattern.is_none());
    let result_roca_type = if let Some(arm) = default_arm {
        let kind = infer_kind(&arm.value, &body.ctx);
        if kind == RocaType::Number { RocaType::Number } else { RocaType::Unknown }
    } else if let Some(first) = arms.first() {
        let kind = infer_kind(&first.value, &body.ctx);
        if kind == RocaType::Number { RocaType::Number } else { RocaType::Unknown }
    } else if is_float {
        RocaType::Number
    } else {
        RocaType::Unknown
    };

    let merge = body.ir.create_block();
    body.ir.append_block_param(merge, &result_roca_type);

    let mut remaining_arms: Vec<_> = arms.iter().collect();
    let default_pos = remaining_arms.iter().position(|a| a.pattern.is_none());
    let default = default_pos.map(|i| remaining_arms.remove(i));

    let scrutinee_slot = body.ir.alloc_var(scrutinee);
    let scr_type = if is_float { RocaType::Number } else { RocaType::Unknown };

    for arm in &remaining_arms {
        match &arm.pattern {
            Some(roca::MatchPattern::Value(pattern)) => {
                let scr = body.ir.load_var(scrutinee_slot, &scr_type);
                let pat_val = emit_expr(body, pattern);
                let cond = if is_float {
                    body.ir.f_eq(scr, pat_val)
                } else if let Some(f) = body.ctx.get_func("__string_eq") {
                    let eq = body.ir.call(*f, &[scr, pat_val]);
                    body.ir.extend_bool(eq)
                } else {
                    body.ir.i_eq(scr, pat_val)
                };

                let then_block = body.ir.create_block();
                let next_block = body.ir.create_block();
                body.ir.brif(cond, then_block, next_block);

                body.ir.switch_to(then_block);
                body.ir.seal(then_block);
                let result = emit_expr(body, &arm.value);
                body.ir.jump_with(merge, result);

                body.ir.switch_to(next_block);
                body.ir.seal(next_block);
            }
            Some(roca::MatchPattern::Variant { variant, bindings, .. }) => {
                let scr = body.ir.load_var(scrutinee_slot, &RocaType::Unknown);

                let zero_idx = body.ir.const_i64(0);
                let tag_ptr = if let Some(&f) = body.ctx.get_func("__struct_get_ptr") {
                    body.ir.call(f, &[scr, zero_idx])
                } else { body.null() };

                let variant_cstr = body.ir.leak_cstr(variant);
                let cond = if let Some(&f) = body.ctx.get_func("__string_eq") {
                    let eq = body.ir.call(f, &[tag_ptr, variant_cstr]);
                    body.ir.extend_bool(eq)
                } else { body.null() };

                let then_block = body.ir.create_block();
                let next_block = body.ir.create_block();
                body.ir.brif(cond, then_block, next_block);

                body.ir.switch_to(then_block);
                body.ir.seal(then_block);

                let scr2 = body.ir.load_var(scrutinee_slot, &RocaType::Unknown);
                for (i, binding) in bindings.iter().enumerate() {
                    let field_idx = body.ir.const_i64((i + 1) as i64);
                    let val = if let Some(&f) = body.ctx.get_func("__struct_get_f64") {
                        body.ir.call(f, &[scr2, field_idx])
                    } else { body.number(0.0) };
                    let slot = body.ir.alloc_var(val);
                    body.ctx.set_var_kind(binding.clone(), slot.0, roca_cranelift::cranelift_type::CraneliftType::to_cranelift(&RocaType::Number), RocaType::Number);
                }

                let result = emit_expr(body, &arm.value);
                body.ir.jump_with(merge, result);

                body.ir.switch_to(next_block);
                body.ir.seal(next_block);
            }
            None => {}
        }
    }

    let default_val = if let Some(arm) = default {
        emit_expr(body, &arm.value)
    } else {
        body.ir.default_for(&result_roca_type)
    };
    body.ir.jump_with(merge, default_val);

    body.ir.switch_to(merge);
    body.ir.seal(merge);
    body.ir.block_param(merge, 0)
}

fn emit_field_access(body: &mut Body, target: &Expr, field: &str) -> Value {
    let kind = target_kind(target, &mut body.ctx);

    if let Expr::Ident(name) = target {
        if body.ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&field.to_string())) {
            return super::methods::emit_enum_variant(body, field, &[]);
        }
    }

    let var_name = match target {
        Expr::Ident(name) => Some(name.as_str()),
        Expr::SelfRef => Some("self"),
        _ => None,
    };
    if let Some(var_name) = var_name {
        if let Some(struct_name) = body.ctx.var_struct_type.get(var_name).cloned() {
            if let Some(layout) = body.ctx.struct_layouts.get(&struct_name) {
                if let Some(idx) = layout.field_index(field) {
                    let field_kind = layout.field_kind(field);
                    let obj = emit_expr(body, target);
                    let idx_val = body.ir.const_i64(idx as i64);
                    return if field_kind == RocaType::Number {
                        if let Some(f) = body.ctx.get_func("__struct_get_f64") { body.ir.call(*f, &[obj, idx_val]) }
                        else { body.number(0.0) }
                    } else {
                        if let Some(f) = body.ctx.get_func("__struct_get_ptr") { body.ir.call(*f, &[obj, idx_val]) }
                        else { body.null() }
                    };
                }
            }
        }
    }

    let obj = emit_expr(body, target);
    match field {
        "length" | "len" => emit_length(body.ir, obj, kind, &mut body.ctx),
        _ => obj,
    }
}

fn emit_array_literal(body: &mut Body, elements: &[Expr]) -> Value {
    let arr = if let Some(f) = body.ctx.get_func("__array_new") {
        body.ir.call(*f, &[])
    } else {
        return body.null();
    };

    for elem in elements {
        let val = emit_expr(body, elem);
        emit_array_push(body.ir, arr, val, &mut body.ctx);
    }
    arr
}

fn emit_index(body: &mut Body, target: &Expr, index: &Expr) -> Value {
    let arr = emit_expr(body, target);
    let idx = emit_expr(body, index);
    let idx_i64 = body.ir.to_i64(idx);
    if let Some(f) = body.ctx.get_func("__array_get_f64") {
        body.ir.call(*f, &[arr, idx_i64])
    } else {
        body.number(0.0)
    }
}

fn emit_call(body: &mut Body, target: &Expr, args: &[Expr]) -> Value {
    if let Expr::FieldAccess { target: obj, field } = target {
        return super::methods::emit_method_call(body, obj, field, args);
    }

    if let Expr::Ident(name) = target {
        if name == "log" {
            if let Some(arg) = args.first() {
                let val = emit_expr(body, arg);
                if body.ir.is_number(val) {
                    if let Some(&f) = body.ctx.get_func("__print_f64") { body.ir.call_void(f, &[val]); }
                } else if body.ir.value_ir_type(val) == cranelift_codegen::ir::types::I8 {
                    if let Some(&f) = body.ctx.get_func("__print_bool") { body.ir.call_void(f, &[val]); }
                } else {
                    if let Some(&f) = body.ctx.get_func("__print") { body.ir.call_void(f, &[val]); }
                }
            }
            return body.bool_val(false);
        }
        if let Some(&func_ref) = body.ctx.get_func(name) {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();

            if let Some(handler) = body.ctx.crash_handlers.get(name).cloned() {
                let arg_slots: Vec<_> = arg_vals.iter().map(|v| body.ir.alloc_var(*v)).collect();
                let arg_types: Vec<_> = arg_vals.iter().map(|v| body.ir.value_ir_type(*v)).collect();
                return super::methods::emit_crash_call(body, func_ref, &arg_slots, &arg_types, &handler);
            }

            let results = body.ir.call_multi(func_ref, &arg_vals);
            if !results.is_empty() { return results[0]; }
        }

        if let Some(var) = body.ctx.get_var(name) {
            if var.cranelift_type == cranelift_codegen::ir::types::I64 {
                let func_ptr = body.ir.raw().ins().stack_load(cranelift_codegen::ir::types::I64, var.slot, 0);
                let sig_ref = body.ir.closure_signature(args.len());
                let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();
                let results = body.ir.call_indirect(sig_ref, func_ptr, &arg_vals);
                if !results.is_empty() { return results[0]; }
            }
        }
    }
    body.null()
}

fn emit_closure(body: &mut Body, params: &[String], closure_body: &Expr) -> Value {
    let closure_name = format!("__closure_{}_{}", params.len(), closure_hash(params, closure_body));
    if let Some(&func_ref) = body.ctx.get_func(&closure_name) {
        return body.ir.func_addr(func_ref);
    }
    body.null()
}

/// Simple hash for identifying closures by their AST structure
pub fn closure_hash(params: &[String], body: &Expr) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    for p in params { p.hash(&mut h); }
    h.finish() ^ super::compile::expr_debug_hash(body)
}
