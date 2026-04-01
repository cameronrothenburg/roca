//! Statement emission — Roca statements to Cranelift IR.

use cranelift_codegen::ir::{self, types, Value, InstBuilder};
use cranelift_frontend::FunctionBuilder;

use crate::ast::{self as roca, Expr, Stmt};
use crate::native::helpers::{alloc_slot, load_slot, call_rt, call_void, default_for_ir_type};
use super::context::{EmitCtx, ValKind};
use super::helpers::{
    infer_kind, emit_scope_cleanup, emit_free_by_kind, emit_loop_body_cleanup,
    emit_struct_set,
};
use super::expr::emit_expr;

pub fn emit_stmt(b: &mut FunctionBuilder, stmt: &Stmt, ctx: &mut EmitCtx, returned: &mut bool) {
    match stmt {
        Stmt::Const { name, value, .. } | Stmt::Let { name, value, .. } => {
            let kind = infer_kind(value, ctx);
            if let Expr::StructLit { name: struct_name, .. } = value {
                ctx.var_struct_type.insert(name.clone(), struct_name.clone());
            }
            let val = emit_expr(b, value, ctx);
            let cl_type = b.func.dfg.value_type(val);
            let slot = alloc_slot(b, val);
            ctx.set_var_kind(name.clone(), slot, cl_type, kind);
        }
        Stmt::Return(expr) => {
            let skip = if let Expr::Ident(name) = expr { Some(name.as_str()) } else { None };
            let val = emit_expr(b, expr, ctx);
            emit_scope_cleanup(b, ctx, skip);
            if ctx.returns_err {
                let no_err = b.ins().iconst(types::I8, 0);
                b.ins().return_(&[val, no_err]);
            } else {
                b.ins().return_(&[val]);
            }
            *returned = true;
        }
        Stmt::Expr(expr) => { emit_expr(b, expr, ctx); }
        Stmt::If { condition, then_body, else_body, .. } => {
            emit_if(b, condition, then_body, else_body.as_deref(), ctx, returned);
        }
        Stmt::While { condition, body, .. } => {
            emit_while(b, condition, body, ctx, returned);
        }
        Stmt::For { binding, iter, body } => {
            emit_for(b, binding, iter, body, ctx, returned);
        }
        Stmt::Break => {
            if let Some(exit) = ctx.loop_exit {
                emit_loop_body_cleanup(b, ctx);
                b.ins().jump(exit, &[]);
                *returned = true;
            }
        }
        Stmt::Continue => {
            if let Some(header) = ctx.loop_header {
                emit_loop_body_cleanup(b, ctx);
                b.ins().jump(header, &[]);
                *returned = true;
            }
        }
        Stmt::Assign { name, value } => {
            if let Some(var) = ctx.get_var(name) {
                let slot = var.slot;
                let is_heap = var.is_heap;
                let cl_type = var.cranelift_type;
                let kind = var.kind;
                if is_heap {
                    let rc_release = ctx.func_refs.get("__rc_release").copied();
                    let free_array = ctx.func_refs.get("__free_array").copied();
                    let free_struct = ctx.func_refs.get("__free_struct").copied();
                    let box_free = ctx.func_refs.get("__box_free").copied();
                    emit_free_by_kind(b, slot, cl_type, kind, rc_release, free_array, free_struct, box_free);
                }
                let val = emit_expr(b, value, ctx);
                b.ins().stack_store(val, slot, 0);
            }
        }
        Stmt::FieldAssign { target, field, value } => {
            emit_field_assign(b, target, field, value, ctx);
        }
        Stmt::LetResult { name, err_name, value } => {
            emit_let_result(b, name, err_name, value, ctx);
        }
        Stmt::ReturnErr { name, .. } => {
            if ctx.returns_err {
                emit_scope_cleanup(b, ctx, None);
                let tag = (name.bytes().fold(1u8, |a, c| a.wrapping_add(c))).max(1);
                let default_val = default_for_ir_type(b, ctx.return_type);
                let err_tag = b.ins().iconst(types::I8, tag as i64);
                b.ins().return_(&[default_val, err_tag]);
                *returned = true;
            }
        }
        Stmt::Wait { names, failed_name, kind } => {
            emit_wait(b, names, failed_name, kind, ctx);
        }
    }
}

fn emit_if(
    b: &mut FunctionBuilder,
    condition: &Expr,
    then_body: &[Stmt],
    else_body: Option<&[Stmt]>,
    ctx: &mut EmitCtx,
    _returned: &mut bool,
) {
    let cond = emit_expr(b, condition, ctx);
    let then_block = b.create_block();
    let else_block = b.create_block();
    let merge_block = b.create_block();
    b.ins().brif(cond, then_block, &[], else_block, &[]);

    let heap_base = ctx.live_heap_vars.len();
    let saved_vars = ctx.vars.clone();
    let saved_struct_types = ctx.var_struct_type.clone();

    b.switch_to_block(then_block);
    b.seal_block(then_block);
    let mut then_ret = false;
    for s in then_body { if then_ret { break; } emit_stmt(b, s, ctx, &mut then_ret); }
    if !then_ret { b.ins().jump(merge_block, &[]); }

    ctx.live_heap_vars.truncate(heap_base);
    ctx.vars = saved_vars.clone();
    ctx.var_struct_type = saved_struct_types.clone();

    b.switch_to_block(else_block);
    b.seal_block(else_block);
    let mut else_ret = false;
    if let Some(body) = else_body {
        for s in body { if else_ret { break; } emit_stmt(b, s, ctx, &mut else_ret); }
    }
    if !else_ret { b.ins().jump(merge_block, &[]); }

    ctx.live_heap_vars.truncate(heap_base);
    ctx.vars = saved_vars;
    ctx.var_struct_type = saved_struct_types;

    b.switch_to_block(merge_block);
    b.seal_block(merge_block);
}

fn emit_while(
    b: &mut FunctionBuilder,
    condition: &Expr,
    body: &[Stmt],
    ctx: &mut EmitCtx,
    _returned: &mut bool,
) {
    let header = b.create_block();
    let body_block = b.create_block();
    let exit = b.create_block();

    let prev_exit = ctx.loop_exit.replace(exit);
    let prev_header = ctx.loop_header.replace(header);
    let prev_heap_base = ctx.loop_heap_base;
    ctx.loop_heap_base = ctx.live_heap_vars.len();

    b.ins().jump(header, &[]);
    b.switch_to_block(header);
    let cond = emit_expr(b, condition, ctx);
    b.ins().brif(cond, body_block, &[], exit, &[]);

    b.switch_to_block(body_block);
    b.seal_block(body_block);
    let mut body_ret = false;
    for s in body { if body_ret { break; } emit_stmt(b, s, ctx, &mut body_ret); }
    if !body_ret {
        emit_loop_body_cleanup(b, ctx);
        b.ins().jump(header, &[]);
    }
    b.seal_block(header);

    b.switch_to_block(exit);
    b.seal_block(exit);

    ctx.live_heap_vars.truncate(ctx.loop_heap_base);
    ctx.loop_heap_base = prev_heap_base;
    ctx.loop_exit = prev_exit;
    ctx.loop_header = prev_header;
}

fn emit_for(
    b: &mut FunctionBuilder,
    binding: &str,
    iter: &Expr,
    body: &[Stmt],
    ctx: &mut EmitCtx,
    _returned: &mut bool,
) {
    let arr = emit_expr(b, iter, ctx);
    let len_ref = ctx.get_func("__array_len").copied();

    let len = if let Some(f) = len_ref {
        call_rt(b, f, &[arr])
    } else {
        if b.func.dfg.value_type(arr) == types::F64 {
            b.ins().fcvt_to_sint(types::I64, arr)
        } else {
            arr
        }
    };

    let arr_slot = alloc_slot(b, arr);
    let len_slot = alloc_slot(b, len);
    let zero_i64 = b.ins().iconst(types::I64, 0);
    let idx_slot = alloc_slot(b, zero_i64);

    let header = b.create_block();
    let body_block = b.create_block();
    let exit = b.create_block();

    let prev_exit = ctx.loop_exit.replace(exit);
    let prev_header = ctx.loop_header.replace(header);
    let prev_heap_base = ctx.loop_heap_base;
    ctx.loop_heap_base = ctx.live_heap_vars.len();

    b.ins().jump(header, &[]);
    b.switch_to_block(header);

    let idx = load_slot(b, idx_slot, types::I64);
    let len_val = load_slot(b, len_slot, types::I64);
    let cond = b.ins().icmp(ir::condcodes::IntCC::SignedLessThan, idx, len_val);
    b.ins().brif(cond, body_block, &[], exit, &[]);

    b.switch_to_block(body_block);
    b.seal_block(body_block);

    let idx_val = load_slot(b, idx_slot, types::I64);
    let cur_arr = load_slot(b, arr_slot, types::I64);
    if let Some(f) = ctx.get_func("__array_get_f64") {
        let elem = call_rt(b, *f, &[cur_arr, idx_val]);
        let elem_slot = alloc_slot(b, elem);
        ctx.set_var_kind(binding.to_string(), elem_slot, types::F64, ValKind::Number);
    } else {
        let idx_f = b.ins().fcvt_from_sint(types::F64, idx_val);
        let elem_slot = alloc_slot(b, idx_f);
        ctx.set_var(binding.to_string(), elem_slot, types::F64);
    }

    let mut body_ret = false;
    for s in body { if body_ret { break; } emit_stmt(b, s, ctx, &mut body_ret); }

    if !body_ret {
        emit_loop_body_cleanup(b, ctx);
        let cur = load_slot(b, idx_slot, types::I64);
        let one = b.ins().iconst(types::I64, 1);
        let next = b.ins().iadd(cur, one);
        b.ins().stack_store(next, idx_slot, 0);
        b.ins().jump(header, &[]);
    }
    b.seal_block(header);

    b.switch_to_block(exit);
    b.seal_block(exit);

    ctx.live_heap_vars.truncate(ctx.loop_heap_base);
    ctx.loop_heap_base = prev_heap_base;
    ctx.loop_exit = prev_exit;
    ctx.loop_header = prev_header;
}

fn emit_field_assign(
    b: &mut FunctionBuilder,
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
                        load_slot(b, var.slot, var.cranelift_type)
                    } else { return; };
                    let val = emit_expr(b, value, ctx);
                    let idx_val = b.ins().iconst(types::I64, idx as i64);
                    emit_struct_set(b, obj, idx_val, val, ctx);
                }
            }
        }
    }
}

fn emit_let_result(
    b: &mut FunctionBuilder,
    name: &str,
    err_name: &str,
    value: &Expr,
    ctx: &mut EmitCtx,
) {
    if let Expr::Call { target, args } = value {
        if let Expr::Ident(fn_name) = target.as_ref() {
            if let Some(func_ref) = ctx.get_func(fn_name) {
                let func_ref = *func_ref;
                let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(b, a, ctx)).collect();
                let call = b.ins().call(func_ref, &arg_vals);
                let results = b.inst_results(call).to_vec();
                if results.len() >= 2 {
                    let val = results[0];
                    let err = results[1];
                    let cl_type = b.func.dfg.value_type(val);
                    let val_slot = alloc_slot(b, val);
                    let kind = if cl_type == types::F64 { ValKind::Number } else { ValKind::Other };
                    ctx.set_var_kind(name.to_string(), val_slot, cl_type, kind);
                    let err_slot = alloc_slot(b, err);
                    ctx.set_var_kind(err_name.to_string(), err_slot, types::I8, ValKind::Bool);
                } else if !results.is_empty() {
                    let val = results[0];
                    let cl_type = b.func.dfg.value_type(val);
                    let val_slot = alloc_slot(b, val);
                    ctx.set_var(name.to_string(), val_slot, cl_type);
                    let zero = b.ins().iconst(types::I8, 0);
                    let err_slot = alloc_slot(b, zero);
                    ctx.set_var_kind(err_name.to_string(), err_slot, types::I8, ValKind::Bool);
                }
            }
        }
    }
}

/// Build an array of JIT function pointers for wait expressions.
fn build_wait_fn_array(
    b: &mut FunctionBuilder,
    exprs: &[roca::Expr],
    ctx: &mut EmitCtx,
) -> (Value, Value) {
    let arr = if let Some(&arr_new) = ctx.get_func("__array_new") {
        call_rt(b, arr_new, &[])
    } else {
        b.ins().iconst(types::I64, 0)
    };
    for expr in exprs {
        let name = format!("__wait_{}", super::compile::wait_expr_hash(expr));
        if let Some(&func_ref) = ctx.get_func(&name) {
            let ptr = b.ins().func_addr(types::I64, func_ref);
            if let Some(&push) = ctx.get_func("__array_push_str") {
                call_void(b, push, &[arr, ptr]);
            }
        }
    }
    let count = b.ins().iconst(types::I64, exprs.len() as i64);
    (arr, count)
}

fn emit_wait(
    b: &mut FunctionBuilder,
    names: &[String],
    failed_name: &str,
    kind: &roca::WaitKind,
    ctx: &mut EmitCtx,
) {
    match kind {
        roca::WaitKind::Single(expr) => {
            let val = emit_expr(b, expr, ctx);
            let cl_type = b.func.dfg.value_type(val);
            if !names.is_empty() {
                let slot = alloc_slot(b, val);
                let kind = infer_kind(expr, ctx);
                ctx.set_var_kind(names[0].clone(), slot, cl_type, kind);
            }
            let false_val = b.ins().iconst(types::I8, 0);
            let err_slot = alloc_slot(b, false_val);
            ctx.set_var_kind(failed_name.to_string(), err_slot, types::I8, ValKind::Bool);
        }
        roca::WaitKind::All(exprs) => {
            let (arr, count) = build_wait_fn_array(b, exprs, ctx);
            if let Some(&wait_all) = ctx.get_func("__wait_all") {
                let results = call_rt(b, wait_all, &[arr, count]);
                for (i, name) in names.iter().enumerate() {
                    if let Some(&get) = ctx.get_func("__array_get_f64") {
                        let idx = b.ins().iconst(types::I64, i as i64);
                        let val = call_rt(b, get, &[results, idx]);
                        let slot = alloc_slot(b, val);
                        ctx.set_var_kind(name.clone(), slot, types::F64, ValKind::Number);
                    }
                }
            }
            let false_val = b.ins().iconst(types::I8, 0);
            let err_slot = alloc_slot(b, false_val);
            ctx.set_var_kind(failed_name.to_string(), err_slot, types::I8, ValKind::Bool);
        }
        roca::WaitKind::First(exprs) => {
            let (arr, count) = build_wait_fn_array(b, exprs, ctx);
            if let Some(&wait_first) = ctx.get_func("__wait_first") {
                let val = call_rt(b, wait_first, &[arr, count]);
                if !names.is_empty() {
                    let slot = alloc_slot(b, val);
                    ctx.set_var_kind(names[0].clone(), slot, types::F64, ValKind::Number);
                }
            }
            let false_val = b.ins().iconst(types::I8, 0);
            let err_slot = alloc_slot(b, false_val);
            ctx.set_var_kind(failed_name.to_string(), err_slot, types::I8, ValKind::Bool);
        }
    }
}
