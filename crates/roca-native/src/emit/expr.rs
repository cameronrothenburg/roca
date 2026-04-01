//! Expression emission — Roca expressions to Cranelift IR values.

use cranelift_codegen::ir::InstBuilder;
use roca_ast::{self as roca, Expr, BinOp, StringPart};
use roca_types::RocaType;
use roca_cranelift::builder::{IrBuilder, Value, FuncRef};
use roca_cranelift::context::{EmitCtx, ValKind};
use roca_cranelift::cranelift_type::CraneliftType;
use super::helpers::{infer_kind, emit_length, target_kind, emit_array_push};

pub fn emit_expr(ir: &mut IrBuilder, expr: &Expr, ctx: &mut EmitCtx) -> Value {
    match expr {
        Expr::Number(n) => ir.const_number(*n),
        Expr::Bool(v) => ir.const_bool(*v),
        Expr::String(s) => {
            let static_ptr = ir.leak_cstr(s);
            if let Some(&f) = ctx.get_func("__string_new") {
                ir.call(f, &[static_ptr])
            } else {
                static_ptr
            }
        }
        Expr::Ident(name) => {
            if let Some(var) = ctx.get_var(name) {
                ir.raw().ins().stack_load(var.cranelift_type, var.slot, 0)
            } else {
                ir.null()
            }
        }
        Expr::BinOp { left, op, right } => {
            let l_is_temp_string = matches!(op, BinOp::Add)
                && !matches!(left.as_ref(), Expr::Ident(_) | Expr::Number(_) | Expr::Bool(_) | Expr::Null)
                && infer_kind(left, ctx) == RocaType::String;
            let l = emit_expr(ir, left, ctx);
            let r = emit_expr(ir, right, ctx);
            let result = emit_binop(ir, op, l, r, ctx);
            if l_is_temp_string {
                if let Some(&f) = ctx.get_func("__rc_release") {
                    ir.call_void(f, &[l]);
                }
            }
            result
        }
        Expr::StructLit { name, fields } => super::methods::emit_struct_lit(ir, name, fields, ctx),
        Expr::Call { target, args } => emit_call(ir, target, args, ctx),
        Expr::Array(elements) => emit_array_literal(ir, elements, ctx),
        Expr::Index { target, index } => emit_index(ir, target, index, ctx),
        Expr::Not(inner) => {
            let val = emit_expr(ir, inner, ctx);
            let zero = ir.null();
            ir.i_eq(val, zero)
        }
        Expr::Closure { params, body } => emit_closure(ir, params, body, ctx),
        Expr::SelfRef => {
            if let Some(var) = ctx.get_var("self") {
                ir.raw().ins().stack_load(var.cranelift_type, var.slot, 0)
            } else {
                ir.null()
            }
        }
        Expr::Null => ir.null(),
        Expr::StringInterp(parts) => emit_string_interp(ir, parts, ctx),
        Expr::Match { value, arms } => emit_match(ir, value, arms, ctx),
        Expr::FieldAccess { target, field } => emit_field_access(ir, target, field, ctx),
        Expr::EnumVariant { enum_name: _, variant, args } => {
            super::methods::emit_enum_variant(ir, variant, args, ctx)
        }
        Expr::Await(inner) => emit_expr(ir, inner, ctx),
    }
}

fn emit_binop(ir: &mut IrBuilder, op: &BinOp, l: Value, r: Value, ctx: &mut EmitCtx) -> Value {
    let is_float = ir.is_number(l);

    match op {
        BinOp::Add if is_float => ir.add(l, r),
        BinOp::Add => {
            if let Some(f) = ctx.get_func("__string_concat") { ir.call(*f, &[l, r]) }
            else { ir.iadd(l, r) }
        }
        BinOp::Sub => ir.sub(l, r),
        BinOp::Mul => ir.mul(l, r),
        BinOp::Div => ir.div(l, r),
        BinOp::Eq if is_float => ir.f_eq(l, r),
        BinOp::Eq => {
            if let Some(f) = ctx.get_func("__string_eq") {
                let result = ir.call(*f, &[l, r]);
                ir.extend_bool(result)
            } else {
                ir.i_eq(l, r)
            }
        }
        BinOp::Neq if is_float => ir.f_ne(l, r),
        BinOp::Neq => {
            if let Some(f) = ctx.get_func("__string_eq") {
                let eq = ir.call(*f, &[l, r]);
                let ext = ir.extend_bool(eq);
                let one = ir.const_i64(1);
                ir.isub(one, ext)
            } else {
                ir.i_ne(l, r)
            }
        }
        BinOp::Lt => ir.f_lt(l, r),
        BinOp::Gt => ir.f_gt(l, r),
        BinOp::Lte => ir.f_le(l, r),
        BinOp::Gte => ir.f_ge(l, r),
        BinOp::And => ir.bool_and(l, r),
        BinOp::Or => ir.bool_or(l, r),
    }
}

fn emit_string_interp(ir: &mut IrBuilder, parts: &[StringPart], ctx: &mut EmitCtx) -> Value {
    let concat = ctx.get_func("__string_concat").copied();
    let to_str = ctx.get_func("__string_from_f64").copied();
    let string_new = ctx.get_func("__string_new").copied();

    let mut result: Option<Value> = None;
    for part in parts {
        let val = match part {
            StringPart::Literal(s) => {
                let static_ptr = ir.leak_cstr(s);
                if let Some(f) = string_new { ir.call(f, &[static_ptr]) } else { static_ptr }
            }
            StringPart::Expr(expr) => {
                let v = emit_expr(ir, expr, ctx);
                if ir.is_number(v) {
                    if let Some(f) = to_str { ir.call(f, &[v]) } else { v }
                } else {
                    v
                }
            }
        };
        result = Some(match result {
            None => val,
            Some(acc) => {
                if let Some(f) = concat { ir.call(f, &[acc, val]) } else { val }
            }
        });
    }
    result.unwrap_or_else(|| ir.null())
}

fn emit_match(ir: &mut IrBuilder, value: &Expr, arms: &[roca::MatchArm], ctx: &mut EmitCtx) -> Value {
    let scrutinee = emit_expr(ir, value, ctx);
    let is_float = ir.is_number(scrutinee);

    let default_arm = arms.iter().find(|a| a.pattern.is_none());
    let result_roca_type = if let Some(arm) = default_arm {
        let kind = infer_kind(&arm.value, ctx);
        if kind == RocaType::Number { RocaType::Number } else { RocaType::Unknown }
    } else if let Some(first) = arms.first() {
        let kind = infer_kind(&first.value, ctx);
        if kind == RocaType::Number { RocaType::Number } else { RocaType::Unknown }
    } else if is_float {
        RocaType::Number
    } else {
        RocaType::Unknown
    };

    let merge = ir.create_block();
    ir.append_block_param(merge, &result_roca_type);

    let mut remaining_arms: Vec<_> = arms.iter().collect();
    let default_pos = remaining_arms.iter().position(|a| a.pattern.is_none());
    let default = default_pos.map(|i| remaining_arms.remove(i));

    let scrutinee_slot = ir.alloc_var(scrutinee);
    let scr_type = if is_float { RocaType::Number } else { RocaType::Unknown };

    for arm in &remaining_arms {
        match &arm.pattern {
            Some(roca::MatchPattern::Value(pattern)) => {
                let scr = ir.load_var(scrutinee_slot, &scr_type);
                let pat_val = emit_expr(ir, pattern, ctx);
                let cond = if is_float {
                    ir.f_eq(scr, pat_val)
                } else if let Some(f) = ctx.get_func("__string_eq") {
                    let eq = ir.call(*f, &[scr, pat_val]);
                    ir.extend_bool(eq)
                } else {
                    ir.i_eq(scr, pat_val)
                };

                let then_block = ir.create_block();
                let next_block = ir.create_block();
                ir.brif(cond, then_block, next_block);

                ir.switch_to(then_block);
                ir.seal(then_block);
                let result = emit_expr(ir, &arm.value, ctx);
                ir.jump_with(merge, result);

                ir.switch_to(next_block);
                ir.seal(next_block);
            }
            Some(roca::MatchPattern::Variant { variant, bindings, .. }) => {
                let scr = ir.load_var(scrutinee_slot, &RocaType::Unknown);

                let zero_idx = ir.const_i64(0);
                let tag_ptr = if let Some(&f) = ctx.get_func("__struct_get_ptr") {
                    ir.call(f, &[scr, zero_idx])
                } else { ir.null() };

                let variant_cstr = ir.leak_cstr(variant);
                let cond = if let Some(&f) = ctx.get_func("__string_eq") {
                    let eq = ir.call(f, &[tag_ptr, variant_cstr]);
                    ir.extend_bool(eq)
                } else { ir.null() };

                let then_block = ir.create_block();
                let next_block = ir.create_block();
                ir.brif(cond, then_block, next_block);

                ir.switch_to(then_block);
                ir.seal(then_block);

                let scr2 = ir.load_var(scrutinee_slot, &RocaType::Unknown);
                for (i, binding) in bindings.iter().enumerate() {
                    let field_idx = ir.const_i64((i + 1) as i64);
                    let val = if let Some(&f) = ctx.get_func("__struct_get_f64") {
                        ir.call(f, &[scr2, field_idx])
                    } else { ir.const_number(0.0) };
                    let slot = ir.alloc_var(val);
                    ctx.set_var_kind(binding.clone(), slot.0, roca_cranelift::cranelift_type::CraneliftType::to_cranelift(&RocaType::Number), RocaType::Number);
                }

                let result = emit_expr(ir, &arm.value, ctx);
                ir.jump_with(merge, result);

                ir.switch_to(next_block);
                ir.seal(next_block);
            }
            None => {}
        }
    }

    let default_val = if let Some(arm) = default {
        emit_expr(ir, &arm.value, ctx)
    } else {
        ir.default_for(&result_roca_type)
    };
    ir.jump_with(merge, default_val);

    ir.switch_to(merge);
    ir.seal(merge);
    ir.block_param(merge, 0)
}

fn emit_field_access(ir: &mut IrBuilder, target: &Expr, field: &str, ctx: &mut EmitCtx) -> Value {
    let kind = target_kind(target, ctx);

    if let Expr::Ident(name) = target {
        if ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&field.to_string())) {
            return super::methods::emit_enum_variant(ir, field, &[], ctx);
        }
    }

    let var_name = match target {
        Expr::Ident(name) => Some(name.as_str()),
        Expr::SelfRef => Some("self"),
        _ => None,
    };
    if let Some(var_name) = var_name {
        if let Some(struct_name) = ctx.var_struct_type.get(var_name).cloned() {
            if let Some(layout) = ctx.struct_layouts.get(&struct_name) {
                if let Some(idx) = layout.field_index(field) {
                    let field_kind = layout.field_kind(field);
                    let obj = emit_expr(ir, target, ctx);
                    let idx_val = ir.const_i64(idx as i64);
                    return if field_kind == RocaType::Number {
                        if let Some(f) = ctx.get_func("__struct_get_f64") { ir.call(*f, &[obj, idx_val]) }
                        else { ir.const_number(0.0) }
                    } else {
                        if let Some(f) = ctx.get_func("__struct_get_ptr") { ir.call(*f, &[obj, idx_val]) }
                        else { ir.null() }
                    };
                }
            }
        }
    }

    let obj = emit_expr(ir, target, ctx);
    match field {
        "length" | "len" => emit_length(ir, obj, kind, ctx),
        _ => obj,
    }
}

fn emit_array_literal(ir: &mut IrBuilder, elements: &[Expr], ctx: &mut EmitCtx) -> Value {
    let arr = if let Some(f) = ctx.get_func("__array_new") {
        ir.call(*f, &[])
    } else {
        return ir.null();
    };

    for elem in elements {
        let val = emit_expr(ir, elem, ctx);
        emit_array_push(ir, arr, val, ctx);
    }
    arr
}

fn emit_index(ir: &mut IrBuilder, target: &Expr, index: &Expr, ctx: &mut EmitCtx) -> Value {
    let arr = emit_expr(ir, target, ctx);
    let idx = emit_expr(ir, index, ctx);
    let idx_i64 = ir.to_i64(idx);
    if let Some(f) = ctx.get_func("__array_get_f64") {
        ir.call(*f, &[arr, idx_i64])
    } else {
        ir.const_number(0.0)
    }
}

fn emit_call(ir: &mut IrBuilder, target: &Expr, args: &[Expr], ctx: &mut EmitCtx) -> Value {
    if let Expr::FieldAccess { target: obj, field } = target {
        return super::methods::emit_method_call(ir, obj, field, args, ctx);
    }

    if let Expr::Ident(name) = target {
        if name == "log" {
            if let Some(arg) = args.first() {
                let val = emit_expr(ir, arg, ctx);
                if ir.is_number(val) {
                    if let Some(&f) = ctx.get_func("__print_f64") { ir.call_void(f, &[val]); }
                } else if ir.value_ir_type(val) == cranelift_codegen::ir::types::I8 {
                    if let Some(&f) = ctx.get_func("__print_bool") { ir.call_void(f, &[val]); }
                } else {
                    if let Some(&f) = ctx.get_func("__print") { ir.call_void(f, &[val]); }
                }
            }
            return ir.const_bool(false);
        }
        if let Some(&func_ref) = ctx.get_func(name) {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(ir, a, ctx)).collect();

            if let Some(handler) = ctx.crash_handlers.get(name).cloned() {
                let arg_slots: Vec<_> = arg_vals.iter().map(|v| ir.alloc_var(*v)).collect();
                let arg_types: Vec<_> = arg_vals.iter().map(|v| ir.value_ir_type(*v)).collect();
                return super::methods::emit_crash_call(ir, func_ref, &arg_slots, &arg_types, &handler, ctx);
            }

            let results = ir.call_multi(func_ref, &arg_vals);
            if !results.is_empty() { return results[0]; }
        }

        if let Some(var) = ctx.get_var(name) {
            if var.cranelift_type == cranelift_codegen::ir::types::I64 {
                let func_ptr = ir.raw().ins().stack_load(cranelift_codegen::ir::types::I64, var.slot, 0);
                let sig_ref = ir.closure_signature(args.len());
                let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(ir, a, ctx)).collect();
                let results = ir.call_indirect(sig_ref, func_ptr, &arg_vals);
                if !results.is_empty() { return results[0]; }
            }
        }
    }
    ir.null()
}

fn emit_closure(ir: &mut IrBuilder, params: &[String], body: &Expr, ctx: &mut EmitCtx) -> Value {
    let closure_name = format!("__closure_{}_{}", params.len(), closure_hash(params, body));
    if let Some(&func_ref) = ctx.get_func(&closure_name) {
        return ir.func_addr(func_ref);
    }
    ir.null()
}

/// Simple hash for identifying closures by their AST structure
pub fn closure_hash(params: &[String], body: &Expr) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    for p in params { p.hash(&mut h); }
    h.finish() ^ super::compile::expr_debug_hash(body)
}
