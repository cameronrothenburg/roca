//! Statement emission — Roca statements to Cranelift IR.

use cranelift_codegen::ir::InstBuilder;
use roca_ast::{self as roca, Expr, Stmt};
use roca_types::RocaType;
use roca_cranelift::builder::{IrBuilder, Value};
use roca_cranelift::context::{EmitCtx, ValKind};
use roca_cranelift::cranelift_type::CraneliftType;
use super::helpers::{
    infer_kind, emit_scope_cleanup, emit_free_by_kind, emit_loop_body_cleanup,
    emit_struct_set, FreeRefs,
};
use super::expr::emit_expr;

pub fn emit_stmt(ir: &mut IrBuilder, stmt: &Stmt, ctx: &mut EmitCtx, returned: &mut bool) {
    match stmt {
        Stmt::Const { name, value, .. } | Stmt::Let { name, value, .. } => {
            let kind = infer_kind(value, ctx);
            if let Expr::StructLit { name: struct_name, .. } = value {
                ctx.var_struct_type.insert(name.clone(), struct_name.clone());
            }
            let val = emit_expr(ir, value, ctx);
            let cl_type = ir.value_ir_type(val);
            let slot = ir.alloc_var(val);
            ctx.set_var_kind(name.clone(), slot.0, cl_type, kind);
        }
        Stmt::Return(expr) => {
            let skip = if let Expr::Ident(name) = expr { Some(name.as_str()) } else { None };
            let val = emit_expr(ir, expr, ctx);
            emit_scope_cleanup(ir, ctx, skip);
            if ctx.returns_err {
                let no_err = ir.const_bool(false);
                ir.ret_with_err(val, no_err);
            } else {
                ir.ret(val);
            }
            *returned = true;
        }
        Stmt::Expr(expr) => { emit_expr(ir, expr, ctx); }
        Stmt::If { condition, then_body, else_body, .. } => {
            emit_if(ir, condition, then_body, else_body.as_deref(), ctx, returned);
        }
        Stmt::While { condition, body, .. } => {
            emit_while(ir, condition, body, ctx, returned);
        }
        Stmt::For { binding, iter, body } => {
            emit_for(ir, binding, iter, body, ctx, returned);
        }
        Stmt::Break => {
            if let Some(exit) = ctx.loop_exit {
                emit_loop_body_cleanup(ir, ctx);
                ir.raw().ins().jump(exit, &[]);
                *returned = true;
            }
        }
        Stmt::Continue => {
            if let Some(header) = ctx.loop_header {
                emit_loop_body_cleanup(ir, ctx);
                ir.raw().ins().jump(header, &[]);
                *returned = true;
            }
        }
        Stmt::Assign { name, value } => {
            if let Some(var) = ctx.get_var(name) {
                let slot = var.slot;
                let is_heap = var.is_heap;
                let cl_type = var.cranelift_type;
                let kind = var.kind.clone();
                if is_heap {
                    let refs = FreeRefs::from_ctx(ctx);
                    emit_free_by_kind(ir, slot, cl_type, kind, &refs);
                }
                let val = emit_expr(ir, value, ctx);
                ir.raw().ins().stack_store(val, slot, 0);
            }
        }
        Stmt::FieldAssign { target, field, value } => {
            emit_field_assign(ir, target, field, value, ctx);
        }
        Stmt::LetResult { name, err_name, value } => {
            emit_let_result(ir, name, err_name, value, ctx);
        }
        Stmt::ReturnErr { name, .. } => {
            if ctx.returns_err {
                emit_scope_cleanup(ir, ctx, None);
                let default_val = roca_cranelift::helpers::default_for_ir_type(ir.raw(), ctx.return_type);
                let tag = (name.bytes().fold(1u8, |a, c| a.wrapping_add(c))).max(1);
                let err_tag = ir.raw().ins().iconst(cranelift_codegen::ir::types::I8, tag as i64);
                ir.ret_with_err(default_val, err_tag);
                *returned = true;
            }
        }
        Stmt::Wait { names, failed_name, kind } => {
            emit_wait(ir, names, failed_name, kind, ctx);
        }
    }
}

fn emit_if(
    ir: &mut IrBuilder,
    condition: &Expr,
    then_body: &[Stmt],
    else_body: Option<&[Stmt]>,
    ctx: &mut EmitCtx,
    _returned: &mut bool,
) {
    let cond = emit_expr(ir, condition, ctx);
    let then_block = ir.create_block();
    let else_block = ir.create_block();
    let merge_block = ir.create_block();
    ir.brif(cond, then_block, else_block);

    let heap_base = ctx.live_heap_vars.len();
    let saved_vars = ctx.vars.clone();
    let saved_struct_types = ctx.var_struct_type.clone();

    ir.switch_to(then_block);
    ir.seal(then_block);
    let mut then_ret = false;
    for s in then_body { if then_ret { break; } emit_stmt(ir, s, ctx, &mut then_ret); }
    if !then_ret { ir.jump(merge_block); }

    ctx.live_heap_vars.truncate(heap_base);
    ctx.vars = saved_vars.clone();
    ctx.var_struct_type = saved_struct_types.clone();

    ir.switch_to(else_block);
    ir.seal(else_block);
    let mut else_ret = false;
    if let Some(body) = else_body {
        for s in body { if else_ret { break; } emit_stmt(ir, s, ctx, &mut else_ret); }
    }
    if !else_ret { ir.jump(merge_block); }

    ctx.live_heap_vars.truncate(heap_base);
    ctx.vars = saved_vars;
    ctx.var_struct_type = saved_struct_types;

    ir.switch_to(merge_block);
    ir.seal(merge_block);
}

fn emit_while(
    ir: &mut IrBuilder,
    condition: &Expr,
    body: &[Stmt],
    ctx: &mut EmitCtx,
    _returned: &mut bool,
) {
    let header = ir.create_block();
    let body_block = ir.create_block();
    let exit = ir.create_block();

    let prev_exit = ctx.loop_exit.replace(exit.0);
    let prev_header = ctx.loop_header.replace(header.0);
    let prev_heap_base = ctx.loop_heap_base;
    ctx.loop_heap_base = ctx.live_heap_vars.len();

    ir.jump(header);
    ir.switch_to(header);
    let cond = emit_expr(ir, condition, ctx);
    ir.brif(cond, body_block, exit);

    ir.switch_to(body_block);
    ir.seal(body_block);
    let mut body_ret = false;
    for s in body { if body_ret { break; } emit_stmt(ir, s, ctx, &mut body_ret); }
    if !body_ret {
        emit_loop_body_cleanup(ir, ctx);
        ir.jump(header);
    }
    ir.seal(header);

    ir.switch_to(exit);
    ir.seal(exit);

    ctx.live_heap_vars.truncate(ctx.loop_heap_base);
    ctx.loop_heap_base = prev_heap_base;
    ctx.loop_exit = prev_exit;
    ctx.loop_header = prev_header;
}

fn emit_for(
    ir: &mut IrBuilder,
    binding: &str,
    iter: &Expr,
    body: &[Stmt],
    ctx: &mut EmitCtx,
    _returned: &mut bool,
) {
    let arr = emit_expr(ir, iter, ctx);
    let len_ref = ctx.get_func("__array_len").copied();

    let len = if let Some(f) = len_ref {
        ir.call(f, &[arr])
    } else {
        if ir.is_number(arr) {
            ir.f64_to_i64(arr)
        } else {
            arr
        }
    };

    let arr_slot = ir.alloc_var(arr);
    let len_slot = ir.alloc_var(len);
    let zero_i64 = ir.const_i64(0);
    let idx_slot = ir.alloc_var(zero_i64);

    let header = ir.create_block();
    let body_block = ir.create_block();
    let exit = ir.create_block();

    let prev_exit = ctx.loop_exit.replace(exit.0);
    let prev_header = ctx.loop_header.replace(header.0);
    let prev_heap_base = ctx.loop_heap_base;
    ctx.loop_heap_base = ctx.live_heap_vars.len();

    ir.jump(header);
    ir.switch_to(header);

    let idx = ir.load_var(idx_slot, &RocaType::Unknown);
    let len_val = ir.load_var(len_slot, &RocaType::Unknown);
    let cond = ir.i_slt(idx, len_val);
    ir.brif(cond, body_block, exit);

    ir.switch_to(body_block);
    ir.seal(body_block);

    let idx_val = ir.load_var(idx_slot, &RocaType::Unknown);
    let cur_arr = ir.load_var(arr_slot, &RocaType::Unknown);
    if let Some(f) = ctx.get_func("__array_get_f64") {
        let elem = ir.call(*f, &[cur_arr, idx_val]);
        let elem_slot = ir.alloc_var(elem);
        ctx.set_var_kind(binding.to_string(), elem_slot.0, RocaType::Number.to_cranelift(), RocaType::Number);
    } else {
        let idx_f = ir.i64_to_f64(idx_val);
        let elem_slot = ir.alloc_var(idx_f);
        ctx.set_var(binding.to_string(), elem_slot.0, RocaType::Number.to_cranelift());
    }

    let mut body_ret = false;
    for s in body { if body_ret { break; } emit_stmt(ir, s, ctx, &mut body_ret); }

    if !body_ret {
        emit_loop_body_cleanup(ir, ctx);
        let cur = ir.load_var(idx_slot, &RocaType::Unknown);
        let one = ir.const_i64(1);
        let next = ir.iadd(cur, one);
        ir.store_var(idx_slot, next);
        ir.jump(header);
    }
    ir.seal(header);

    ir.switch_to(exit);
    ir.seal(exit);

    ctx.live_heap_vars.truncate(ctx.loop_heap_base);
    ctx.loop_heap_base = prev_heap_base;
    ctx.loop_exit = prev_exit;
    ctx.loop_header = prev_header;
}

fn emit_field_assign(
    ir: &mut IrBuilder,
    target: &Expr,
    field: &str,
    value: &Expr,
    ctx: &mut EmitCtx,
) {
    let var_name = match target {
        Expr::Ident(name) => Some(name.as_str()),
        Expr::SelfRef => Some("self"),
        _ => None,
    };
    if let Some(var_name) = var_name {
        if let Some(struct_name) = ctx.var_struct_type.get(var_name).cloned() {
            if let Some(layout) = ctx.struct_layouts.get(&struct_name) {
                if let Some(idx) = layout.field_index(field) {
                    let obj = if let Some(var) = ctx.get_var(var_name) {
                        ir.raw().ins().stack_load(var.cranelift_type, var.slot, 0)
                    } else { return; };
                    let val = emit_expr(ir, value, ctx);
                    let idx_val = ir.const_i64(idx as i64);
                    emit_struct_set(ir, obj, idx_val, val, ctx);
                }
            }
        }
    }
}

fn emit_let_result(
    ir: &mut IrBuilder,
    name: &str,
    err_name: &str,
    value: &Expr,
    ctx: &mut EmitCtx,
) {
    if let Expr::Call { target, args } = value {
        if let Expr::Ident(fn_name) = target.as_ref() {
            if let Some(func_ref) = ctx.get_func(fn_name) {
                let func_ref = *func_ref;
                let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(ir, a, ctx)).collect();
                let results = ir.call_multi(func_ref, &arg_vals);
                if results.len() >= 2 {
                    let val = results[0];
                    let err = results[1];
                    let cl_type = ir.value_ir_type(val);
                    let val_slot = ir.alloc_var(val);
                    let kind = if ir.is_number(val) { RocaType::Number } else { RocaType::Unknown };
                    ctx.set_var_kind(name.to_string(), val_slot.0, cl_type, kind);
                    let err_slot = ir.alloc_var(err);
                    ctx.set_var_kind(err_name.to_string(), err_slot.0, cranelift_codegen::ir::types::I8, RocaType::Bool);
                } else if !results.is_empty() {
                    let val = results[0];
                    let cl_type = ir.value_ir_type(val);
                    let val_slot = ir.alloc_var(val);
                    ctx.set_var(name.to_string(), val_slot.0, cl_type);
                    let zero = ir.const_bool(false);
                    let err_slot = ir.alloc_var(zero);
                    ctx.set_var_kind(err_name.to_string(), err_slot.0, cranelift_codegen::ir::types::I8, RocaType::Bool);
                }
            }
        }
    }
}

fn build_wait_fn_array(
    ir: &mut IrBuilder,
    exprs: &[roca::Expr],
    ctx: &mut EmitCtx,
) -> (Value, Value) {
    let arr = if let Some(&arr_new) = ctx.get_func("__array_new") {
        ir.call(arr_new, &[])
    } else {
        ir.null()
    };
    for expr in exprs {
        let name = format!("__wait_{}", super::compile::wait_expr_hash(expr));
        if let Some(&func_ref) = ctx.get_func(&name) {
            let ptr = ir.func_addr(func_ref);
            if let Some(&push) = ctx.get_func("__array_push_str") {
                ir.call_void(push, &[arr, ptr]);
            }
        }
    }
    let count = ir.const_i64(exprs.len() as i64);
    (arr, count)
}

fn emit_wait(
    ir: &mut IrBuilder,
    names: &[String],
    failed_name: &str,
    kind: &roca::WaitKind,
    ctx: &mut EmitCtx,
) {
    match kind {
        roca::WaitKind::Single(expr) => {
            let val = emit_expr(ir, expr, ctx);
            let cl_type = ir.value_ir_type(val);
            if !names.is_empty() {
                let slot = ir.alloc_var(val);
                let kind = infer_kind(expr, ctx);
                ctx.set_var_kind(names[0].clone(), slot.0, cl_type, kind);
            }
            let false_val = ir.const_bool(false);
            let err_slot = ir.alloc_var(false_val);
            ctx.set_var_kind(failed_name.to_string(), err_slot.0, cranelift_codegen::ir::types::I8, RocaType::Bool);
        }
        roca::WaitKind::All(exprs) => {
            let (arr, count) = build_wait_fn_array(ir, exprs, ctx);
            if let Some(&wait_all) = ctx.get_func("__wait_all") {
                let results = ir.call(wait_all, &[arr, count]);
                for (i, name) in names.iter().enumerate() {
                    if let Some(&get) = ctx.get_func("__array_get_f64") {
                        let idx = ir.const_i64(i as i64);
                        let val = ir.call(get, &[results, idx]);
                        let slot = ir.alloc_var(val);
                        ctx.set_var_kind(name.clone(), slot.0, RocaType::Number.to_cranelift(), RocaType::Number);
                    }
                }
            }
            let false_val = ir.const_bool(false);
            let err_slot = ir.alloc_var(false_val);
            ctx.set_var_kind(failed_name.to_string(), err_slot.0, cranelift_codegen::ir::types::I8, RocaType::Bool);
        }
        roca::WaitKind::First(exprs) => {
            let (arr, count) = build_wait_fn_array(ir, exprs, ctx);
            if let Some(&wait_first) = ctx.get_func("__wait_first") {
                let val = ir.call(wait_first, &[arr, count]);
                if !names.is_empty() {
                    let slot = ir.alloc_var(val);
                    ctx.set_var_kind(names[0].clone(), slot.0, RocaType::Number.to_cranelift(), RocaType::Number);
                }
            }
            let false_val = ir.const_bool(false);
            let err_slot = ir.alloc_var(false_val);
            ctx.set_var_kind(failed_name.to_string(), err_slot.0, cranelift_codegen::ir::types::I8, RocaType::Bool);
        }
    }
}
