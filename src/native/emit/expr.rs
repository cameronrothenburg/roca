//! Expression emission — Roca expressions to Cranelift IR values.

use cranelift_codegen::ir::{self, types, AbiParam, Value, BlockArg, InstBuilder};
use cranelift_frontend::FunctionBuilder;

use crate::ast::{self as roca, Expr, BinOp, StringPart};
use crate::native::helpers::{
    fcmp_to_i64, icmp_to_i64, call_rt, call_void, alloc_slot, load_slot,
    bool_and, bool_or, ensure_i64, leak_cstr,
};
use super::context::{EmitCtx, ValKind};
use super::helpers::{infer_kind, emit_length, target_kind, emit_array_push};

pub fn emit_expr(b: &mut FunctionBuilder, expr: &Expr, ctx: &mut EmitCtx) -> Value {
    match expr {
        Expr::Number(n) => b.ins().f64const(*n),
        Expr::Bool(v) => b.ins().iconst(types::I8, if *v { 1 } else { 0 }),
        Expr::String(s) => {
            let static_ptr = leak_cstr(b, s);
            if let Some(&f) = ctx.get_func("__string_new") {
                call_rt(b, f, &[static_ptr])
            } else {
                static_ptr
            }
        }
        Expr::Ident(name) => {
            if let Some(var) = ctx.get_var(name) {
                load_slot(b, var.slot, var.cranelift_type)
            } else {
                b.ins().iconst(types::I64, 0)
            }
        }
        Expr::BinOp { left, op, right } => {
            let l_is_temp_string = matches!(op, BinOp::Add)
                && !matches!(left.as_ref(), Expr::Ident(_) | Expr::Number(_) | Expr::Bool(_) | Expr::Null)
                && infer_kind(left, ctx) == ValKind::String;
            let l = emit_expr(b, left, ctx);
            let r = emit_expr(b, right, ctx);
            let result = emit_binop(b, op, l, r, ctx);
            if l_is_temp_string {
                if let Some(&f) = ctx.get_func("__rc_release") {
                    call_void(b, f, &[l]);
                }
            }
            result
        }
        Expr::StructLit { name, fields } => super::methods::emit_struct_lit(b, name, fields, ctx),
        Expr::Call { target, args } => emit_call(b, target, args, ctx),
        Expr::Array(elements) => emit_array_literal(b, elements, ctx),
        Expr::Index { target, index } => emit_index(b, target, index, ctx),
        Expr::Not(inner) => {
            let val = emit_expr(b, inner, ctx);
            let zero = b.ins().iconst(types::I64, 0);
            icmp_to_i64(b, ir::condcodes::IntCC::Equal, val, zero)
        }
        Expr::Closure { params, body } => emit_closure(b, params, body, ctx),
        Expr::SelfRef => {
            if let Some(var) = ctx.get_var("self") {
                load_slot(b, var.slot, var.cranelift_type)
            } else {
                b.ins().iconst(types::I64, 0)
            }
        }
        Expr::Null => b.ins().iconst(types::I64, 0),
        Expr::StringInterp(parts) => emit_string_interp(b, parts, ctx),
        Expr::Match { value, arms } => emit_match(b, value, arms, ctx),
        Expr::FieldAccess { target, field } => emit_field_access(b, target, field, ctx),
        Expr::EnumVariant { enum_name: _, variant, args } => {
            super::methods::emit_enum_variant(b, variant, args, ctx)
        }
        Expr::Await(inner) => emit_expr(b, inner, ctx),
    }
}

fn emit_binop(b: &mut FunctionBuilder, op: &BinOp, l: Value, r: Value, ctx: &mut EmitCtx) -> Value {
    let is_float = b.func.dfg.value_type(l) == types::F64;
    use ir::condcodes::FloatCC;

    match op {
        BinOp::Add if is_float => b.ins().fadd(l, r),
        BinOp::Add => {
            if let Some(f) = ctx.get_func("__string_concat") { call_rt(b, *f, &[l, r]) }
            else { b.ins().iadd(l, r) }
        }
        BinOp::Sub => b.ins().fsub(l, r),
        BinOp::Mul => b.ins().fmul(l, r),
        BinOp::Div => b.ins().fdiv(l, r),
        BinOp::Eq if is_float => fcmp_to_i64(b, FloatCC::Equal, l, r),
        BinOp::Eq => {
            if let Some(f) = ctx.get_func("__string_eq") {
                let result = call_rt(b, *f, &[l, r]);
                b.ins().uextend(types::I64, result)
            } else {
                icmp_to_i64(b, ir::condcodes::IntCC::Equal, l, r)
            }
        }
        BinOp::Neq if is_float => fcmp_to_i64(b, FloatCC::NotEqual, l, r),
        BinOp::Neq => {
            if let Some(f) = ctx.get_func("__string_eq") {
                let eq = call_rt(b, *f, &[l, r]);
                let ext = b.ins().uextend(types::I64, eq);
                let one = b.ins().iconst(types::I64, 1);
                b.ins().isub(one, ext)
            } else {
                icmp_to_i64(b, ir::condcodes::IntCC::NotEqual, l, r)
            }
        }
        BinOp::Lt => fcmp_to_i64(b, FloatCC::LessThan, l, r),
        BinOp::Gt => fcmp_to_i64(b, FloatCC::GreaterThan, l, r),
        BinOp::Lte => fcmp_to_i64(b, FloatCC::LessThanOrEqual, l, r),
        BinOp::Gte => fcmp_to_i64(b, FloatCC::GreaterThanOrEqual, l, r),
        BinOp::And => bool_and(b, l, r),
        BinOp::Or => bool_or(b, l, r),
    }
}

fn emit_string_interp(b: &mut FunctionBuilder, parts: &[StringPart], ctx: &mut EmitCtx) -> Value {
    let concat = ctx.get_func("__string_concat").copied();
    let to_str = ctx.get_func("__string_from_f64").copied();
    let string_new = ctx.get_func("__string_new").copied();

    let mut result: Option<Value> = None;
    for part in parts {
        let val = match part {
            StringPart::Literal(s) => {
                let static_ptr = leak_cstr(b, s);
                if let Some(f) = string_new { call_rt(b, f, &[static_ptr]) } else { static_ptr }
            }
            StringPart::Expr(expr) => {
                let v = emit_expr(b, expr, ctx);
                if b.func.dfg.value_type(v) == types::F64 {
                    if let Some(f) = to_str { call_rt(b, f, &[v]) } else { v }
                } else {
                    v
                }
            }
        };
        result = Some(match result {
            None => val,
            Some(acc) => {
                if let Some(f) = concat { call_rt(b, f, &[acc, val]) } else { val }
            }
        });
    }
    result.unwrap_or_else(|| b.ins().iconst(types::I64, 0))
}

fn emit_match(b: &mut FunctionBuilder, value: &Expr, arms: &[roca::MatchArm], ctx: &mut EmitCtx) -> Value {
    let scrutinee = emit_expr(b, value, ctx);
    let is_float = b.func.dfg.value_type(scrutinee) == types::F64;

    let default_arm = arms.iter().find(|a| a.pattern.is_none());
    let result_type = if let Some(arm) = default_arm {
        let kind = infer_kind(&arm.value, ctx);
        if kind == ValKind::Number { types::F64 } else { types::I64 }
    } else if let Some(first) = arms.first() {
        let kind = infer_kind(&first.value, ctx);
        if kind == ValKind::Number { types::F64 } else { types::I64 }
    } else if is_float {
        types::F64
    } else {
        types::I64
    };

    let merge = b.create_block();
    b.append_block_param(merge, result_type);

    let mut remaining_arms: Vec<_> = arms.iter().collect();
    let default_arm = remaining_arms.iter().position(|a| a.pattern.is_none());
    let default = default_arm.map(|i| remaining_arms.remove(i));

    let scrutinee_slot = alloc_slot(b, scrutinee);

    for arm in &remaining_arms {
        match &arm.pattern {
            Some(roca::MatchPattern::Value(pattern)) => {
                let scr = load_slot(b, scrutinee_slot, if is_float { types::F64 } else { types::I64 });
                let pat_val = emit_expr(b, pattern, ctx);
                let cond = if is_float {
                    let cmp = b.ins().fcmp(ir::condcodes::FloatCC::Equal, scr, pat_val);
                    b.ins().uextend(types::I64, cmp)
                } else if let Some(f) = ctx.get_func("__string_eq") {
                    let eq = call_rt(b, *f, &[scr, pat_val]);
                    b.ins().uextend(types::I64, eq)
                } else {
                    icmp_to_i64(b, ir::condcodes::IntCC::Equal, scr, pat_val)
                };

                let then_block = b.create_block();
                let next_block = b.create_block();
                b.ins().brif(cond, then_block, &[], next_block, &[]);

                b.switch_to_block(then_block);
                b.seal_block(then_block);
                let result = emit_expr(b, &arm.value, ctx);
                b.ins().jump(merge, &[BlockArg::Value(result)]);

                b.switch_to_block(next_block);
                b.seal_block(next_block);
            }
            Some(roca::MatchPattern::Variant { variant, bindings, .. }) => {
                let scr = load_slot(b, scrutinee_slot, types::I64);

                let zero_idx = b.ins().iconst(types::I64, 0);
                let tag_ptr = if let Some(&f) = ctx.get_func("__struct_get_ptr") {
                    call_rt(b, f, &[scr, zero_idx])
                } else { b.ins().iconst(types::I64, 0) };

                let variant_cstr = leak_cstr(b, variant);
                let cond = if let Some(&f) = ctx.get_func("__string_eq") {
                    let eq = call_rt(b, f, &[tag_ptr, variant_cstr]);
                    b.ins().uextend(types::I64, eq)
                } else { b.ins().iconst(types::I64, 0) };

                let then_block = b.create_block();
                let next_block = b.create_block();
                b.ins().brif(cond, then_block, &[], next_block, &[]);

                b.switch_to_block(then_block);
                b.seal_block(then_block);

                let scr2 = load_slot(b, scrutinee_slot, types::I64);
                for (i, binding) in bindings.iter().enumerate() {
                    let field_idx = b.ins().iconst(types::I64, (i + 1) as i64);
                    let val = if let Some(&f) = ctx.get_func("__struct_get_f64") {
                        call_rt(b, f, &[scr2, field_idx])
                    } else { b.ins().f64const(0.0) };
                    let slot = alloc_slot(b, val);
                    ctx.set_var_kind(binding.clone(), slot, types::F64, ValKind::Number);
                }

                let result = emit_expr(b, &arm.value, ctx);
                b.ins().jump(merge, &[BlockArg::Value(result)]);

                b.switch_to_block(next_block);
                b.seal_block(next_block);
            }
            None => {}
        }
    }

    let default_val = if let Some(arm) = default {
        emit_expr(b, &arm.value, ctx)
    } else if is_float {
        b.ins().f64const(0.0)
    } else {
        b.ins().iconst(types::I64, 0)
    };
    b.ins().jump(merge, &[BlockArg::Value(default_val)]);

    b.switch_to_block(merge);
    b.seal_block(merge);
    b.block_params(merge)[0]
}

fn emit_field_access(b: &mut FunctionBuilder, target: &Expr, field: &str, ctx: &mut EmitCtx) -> Value {
    let kind = target_kind(target, ctx);

    if let Expr::Ident(name) = target {
        if ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&field.to_string())) {
            return super::methods::emit_enum_variant(b, field, &[], ctx);
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
                    let obj = emit_expr(b, target, ctx);
                    let idx_val = b.ins().iconst(types::I64, idx as i64);
                    return match field_kind {
                        ValKind::Number => {
                            if let Some(f) = ctx.get_func("__struct_get_f64") { call_rt(b, *f, &[obj, idx_val]) }
                            else { b.ins().f64const(0.0) }
                        }
                        _ => {
                            if let Some(f) = ctx.get_func("__struct_get_ptr") { call_rt(b, *f, &[obj, idx_val]) }
                            else { b.ins().iconst(types::I64, 0) }
                        }
                    };
                }
            }
        }
    }

    let obj = emit_expr(b, target, ctx);
    match field {
        "length" | "len" => emit_length(b, obj, kind, ctx),
        _ => obj,
    }
}

fn emit_array_literal(b: &mut FunctionBuilder, elements: &[Expr], ctx: &mut EmitCtx) -> Value {
    let arr = if let Some(f) = ctx.get_func("__array_new") {
        call_rt(b, *f, &[])
    } else {
        return b.ins().iconst(types::I64, 0);
    };

    for elem in elements {
        let val = emit_expr(b, elem, ctx);
        emit_array_push(b, arr, val, ctx);
    }
    arr
}

fn emit_index(b: &mut FunctionBuilder, target: &Expr, index: &Expr, ctx: &mut EmitCtx) -> Value {
    let arr = emit_expr(b, target, ctx);
    let idx = emit_expr(b, index, ctx);
    let idx_i64 = ensure_i64(b, idx);
    if let Some(f) = ctx.get_func("__array_get_f64") {
        call_rt(b, *f, &[arr, idx_i64])
    } else {
        b.ins().f64const(0.0)
    }
}

fn emit_call(b: &mut FunctionBuilder, target: &Expr, args: &[Expr], ctx: &mut EmitCtx) -> Value {
    if let Expr::FieldAccess { target: obj, field } = target {
        return super::methods::emit_method_call(b, obj, field, args, ctx);
    }

    if let Expr::Ident(name) = target {
        if name == "log" {
            if let Some(arg) = args.first() {
                let val = emit_expr(b, arg, ctx);
                let ty = b.func.dfg.value_type(val);
                if ty == types::F64 {
                    if let Some(&f) = ctx.get_func("__print_f64") { call_void(b, f, &[val]); }
                } else if ty == types::I8 {
                    if let Some(&f) = ctx.get_func("__print_bool") { call_void(b, f, &[val]); }
                } else {
                    if let Some(&f) = ctx.get_func("__print") { call_void(b, f, &[val]); }
                }
            }
            return b.ins().iconst(types::I8, 0);
        }
        if let Some(&func_ref) = ctx.get_func(name) {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(b, a, ctx)).collect();

            if let Some(handler) = ctx.crash_handlers.get(name).cloned() {
                let arg_slots: Vec<_> = arg_vals.iter().map(|v| alloc_slot(b, *v)).collect();
                let arg_types: Vec<_> = arg_vals.iter().map(|v| b.func.dfg.value_type(*v)).collect();
                return super::methods::emit_crash_call(b, func_ref, &arg_slots, &arg_types, &handler, ctx);
            }

            let call = b.ins().call(func_ref, &arg_vals);
            let results = b.inst_results(call).to_vec();
            if results.len() >= 2 { return results[0]; }
            if !results.is_empty() { return results[0]; }
        }

        if let Some(var) = ctx.get_var(name) {
            if var.cranelift_type == types::I64 {
                let func_ptr = load_slot(b, var.slot, types::I64);
                let mut sig = b.func.signature.clone();
                sig.params.clear();
                sig.returns.clear();
                for _ in args {
                    sig.params.push(AbiParam::new(types::F64));
                }
                sig.returns.push(AbiParam::new(types::F64));
                let sig_ref = b.import_signature(sig);
                let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(b, a, ctx)).collect();
                let call = b.ins().call_indirect(sig_ref, func_ptr, &arg_vals);
                let results = b.inst_results(call);
                if !results.is_empty() { return results[0]; }
            }
        }
    }
    b.ins().iconst(types::I64, 0)
}

fn emit_closure(b: &mut FunctionBuilder, params: &[String], body: &Expr, ctx: &mut EmitCtx) -> Value {
    let closure_name = format!("__closure_{}_{}", params.len(), closure_hash(params, body));
    if let Some(&func_ref) = ctx.get_func(&closure_name) {
        return b.ins().func_addr(types::I64, func_ref);
    }
    b.ins().iconst(types::I64, 0)
}

/// Simple hash for identifying closures by their AST structure
pub fn closure_hash(params: &[String], body: &Expr) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    for p in params { p.hash(&mut h); }
    h.finish() ^ super::compile::expr_debug_hash(body)
}
