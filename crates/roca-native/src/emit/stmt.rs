//! Statement emission — Roca statements to Cranelift IR.

use cranelift_codegen::ir::InstBuilder;
use roca_ast::{self as roca, Expr, Stmt};
use roca_types::RocaType;
use roca_cranelift::api::{Body, Value};
use roca_cranelift::context::ValKind;
use roca_cranelift::cranelift_type::CraneliftType;
use super::helpers::{
    infer_kind, emit_scope_cleanup, emit_free_by_kind, emit_loop_body_cleanup,
    emit_struct_set, FreeRefs,
};
use super::expr::emit_expr;

pub fn emit_stmt(body: &mut Body, stmt: &Stmt) {
    match stmt {
        Stmt::Const { name, value, .. } | Stmt::Let { name, value, .. } => {
            let kind = infer_kind(value, &body.ctx);
            if let Expr::StructLit { name: struct_name, .. } = value {
                body.ctx.var_struct_type.insert(name.clone(), struct_name.clone());
            }
            let val = emit_expr(body, value);
            let cl_type = body.ir.value_ir_type(val);
            let slot = body.ir.alloc_var(val);
            body.ctx.set_var_kind(name.clone(), slot.0, cl_type, kind);
        }
        Stmt::Return(expr) => {
            let skip = if let Expr::Ident(name) = expr { Some(name.as_str()) } else { None };
            let val = emit_expr(body, expr);
            emit_scope_cleanup(body.ir, &body.ctx, skip);
            if body.ctx.returns_err {
                let no_err = body.ir.const_bool(false);
                body.ir.ret_with_err(val, no_err);
            } else {
                body.ir.ret(val);
            }
            body.returned = true;
        }
        Stmt::Expr(expr) => { emit_expr(body, expr); }
        Stmt::If { condition, then_body, else_body, .. } => {
            emit_if(body, condition, then_body, else_body.as_deref());
        }
        Stmt::While { condition, body: while_body, .. } => {
            emit_while(body, condition, while_body);
        }
        Stmt::For { binding, iter, body: for_body } => {
            emit_for(body, binding, iter, for_body);
        }
        Stmt::Break => {
            if let Some(exit) = body.ctx.loop_exit {
                emit_loop_body_cleanup(body.ir, &body.ctx);
                body.ir.raw().ins().jump(exit, &[]);
                body.returned = true;
            }
        }
        Stmt::Continue => {
            if let Some(header) = body.ctx.loop_header {
                emit_loop_body_cleanup(body.ir, &body.ctx);
                body.ir.raw().ins().jump(header, &[]);
                body.returned = true;
            }
        }
        Stmt::Assign { name, value } => {
            if let Some(var) = body.ctx.get_var(name) {
                let slot = var.slot;
                let is_heap = var.is_heap;
                let cl_type = var.cranelift_type;
                let kind = var.kind.clone();
                if is_heap {
                    let refs = FreeRefs::from_ctx(&body.ctx);
                    emit_free_by_kind(body.ir, slot, cl_type, kind, &refs);
                }
                let val = emit_expr(body, value);
                body.ir.raw().ins().stack_store(val, slot, 0);
            }
        }
        Stmt::FieldAssign { target, field, value } => {
            emit_field_assign(body, target, field, value);
        }
        Stmt::LetResult { name, err_name, value } => {
            emit_let_result(body, name, err_name, value);
        }
        Stmt::ReturnErr { name, .. } => {
            if body.ctx.returns_err {
                emit_scope_cleanup(body.ir, &body.ctx, None);
                let default_val = roca_cranelift::helpers::default_for_ir_type(body.ir.raw(), body.ctx.return_type);
                let tag = (name.bytes().fold(1u8, |a, c| a.wrapping_add(c))).max(1);
                let err_tag = body.ir.raw().ins().iconst(cranelift_codegen::ir::types::I8, tag as i64);
                body.ir.ret_with_err(default_val, err_tag);
                body.returned = true;
            }
        }
        Stmt::Wait { names, failed_name, kind } => {
            emit_wait(body, names, failed_name, kind);
        }
    }
}

fn emit_if(
    body: &mut Body,
    condition: &Expr,
    then_body: &[Stmt],
    else_body: Option<&[Stmt]>,
) {
    let cond = emit_expr(body, condition);
    let then_block = body.ir.create_block();
    let else_block = body.ir.create_block();
    let merge_block = body.ir.create_block();
    body.ir.brif(cond, then_block, else_block);

    let heap_base = body.ctx.live_heap_vars.len();
    let saved_vars = body.ctx.vars.clone();
    let saved_struct_types = body.ctx.var_struct_type.clone();

    body.ir.switch_to(then_block);
    body.ir.seal(then_block);
    let mut then_ret = false;
    for s in then_body {
        if then_ret { break; }
        emit_stmt(body, s);
        then_ret = body.returned;
        body.returned = false;
    }
    if !then_ret { body.ir.jump(merge_block); }

    body.ctx.live_heap_vars.truncate(heap_base);
    body.ctx.vars = saved_vars.clone();
    body.ctx.var_struct_type = saved_struct_types.clone();

    body.ir.switch_to(else_block);
    body.ir.seal(else_block);
    let mut else_ret = false;
    if let Some(stmts) = else_body {
        for s in stmts {
            if else_ret { break; }
            emit_stmt(body, s);
            else_ret = body.returned;
            body.returned = false;
        }
    }
    if !else_ret { body.ir.jump(merge_block); }

    body.ctx.live_heap_vars.truncate(heap_base);
    body.ctx.vars = saved_vars;
    body.ctx.var_struct_type = saved_struct_types;

    body.ir.switch_to(merge_block);
    body.ir.seal(merge_block);
}

fn emit_while(
    body: &mut Body,
    condition: &Expr,
    while_body: &[Stmt],
) {
    let header = body.ir.create_block();
    let body_block = body.ir.create_block();
    let exit = body.ir.create_block();

    let prev_exit = body.ctx.loop_exit.replace(exit.0);
    let prev_header = body.ctx.loop_header.replace(header.0);
    let prev_heap_base = body.ctx.loop_heap_base;
    body.ctx.loop_heap_base = body.ctx.live_heap_vars.len();

    body.ir.jump(header);
    body.ir.switch_to(header);
    let cond = emit_expr(body, condition);
    body.ir.brif(cond, body_block, exit);

    body.ir.switch_to(body_block);
    body.ir.seal(body_block);
    let mut body_ret = false;
    for s in while_body {
        if body_ret { break; }
        emit_stmt(body, s);
        body_ret = body.returned;
        body.returned = false;
    }
    if !body_ret {
        emit_loop_body_cleanup(body.ir, &body.ctx);
        body.ir.jump(header);
    }
    body.ir.seal(header);

    body.ir.switch_to(exit);
    body.ir.seal(exit);

    body.ctx.live_heap_vars.truncate(body.ctx.loop_heap_base);
    body.ctx.loop_heap_base = prev_heap_base;
    body.ctx.loop_exit = prev_exit;
    body.ctx.loop_header = prev_header;
}

fn emit_for(
    body: &mut Body,
    binding: &str,
    iter: &Expr,
    for_body: &[Stmt],
) {
    let arr = emit_expr(body, iter);
    let len_ref = body.ctx.get_func("__array_len").copied();

    let len = if let Some(f) = len_ref {
        body.ir.call(f, &[arr])
    } else {
        if body.ir.is_number(arr) {
            body.ir.f64_to_i64(arr)
        } else {
            arr
        }
    };

    let arr_slot = body.ir.alloc_var(arr);
    let len_slot = body.ir.alloc_var(len);
    let zero_i64 = body.ir.const_i64(0);
    let idx_slot = body.ir.alloc_var(zero_i64);

    let header = body.ir.create_block();
    let body_block = body.ir.create_block();
    let exit = body.ir.create_block();

    let prev_exit = body.ctx.loop_exit.replace(exit.0);
    let prev_header = body.ctx.loop_header.replace(header.0);
    let prev_heap_base = body.ctx.loop_heap_base;
    body.ctx.loop_heap_base = body.ctx.live_heap_vars.len();

    body.ir.jump(header);
    body.ir.switch_to(header);

    let idx = body.ir.load_var(idx_slot, &RocaType::Unknown);
    let len_val = body.ir.load_var(len_slot, &RocaType::Unknown);
    let cond = body.ir.i_slt(idx, len_val);
    body.ir.brif(cond, body_block, exit);

    body.ir.switch_to(body_block);
    body.ir.seal(body_block);

    let idx_val = body.ir.load_var(idx_slot, &RocaType::Unknown);
    let cur_arr = body.ir.load_var(arr_slot, &RocaType::Unknown);
    if let Some(f) = body.ctx.get_func("__array_get_f64") {
        let elem = body.ir.call(*f, &[cur_arr, idx_val]);
        let elem_slot = body.ir.alloc_var(elem);
        body.ctx.set_var_kind(binding.to_string(), elem_slot.0, RocaType::Number.to_cranelift(), RocaType::Number);
    } else {
        let idx_f = body.ir.i64_to_f64(idx_val);
        let elem_slot = body.ir.alloc_var(idx_f);
        body.ctx.set_var(binding.to_string(), elem_slot.0, RocaType::Number.to_cranelift());
    }

    let mut body_ret = false;
    for s in for_body {
        if body_ret { break; }
        emit_stmt(body, s);
        body_ret = body.returned;
        body.returned = false;
    }

    if !body_ret {
        emit_loop_body_cleanup(body.ir, &body.ctx);
        let cur = body.ir.load_var(idx_slot, &RocaType::Unknown);
        let one = body.ir.const_i64(1);
        let next = body.ir.iadd(cur, one);
        body.ir.store_var(idx_slot, next);
        body.ir.jump(header);
    }
    body.ir.seal(header);

    body.ir.switch_to(exit);
    body.ir.seal(exit);

    body.ctx.live_heap_vars.truncate(body.ctx.loop_heap_base);
    body.ctx.loop_heap_base = prev_heap_base;
    body.ctx.loop_exit = prev_exit;
    body.ctx.loop_header = prev_header;
}

fn emit_field_assign(
    body: &mut Body,
    target: &Expr,
    field: &str,
    value: &Expr,
) {
    let var_name = match target {
        Expr::Ident(name) => Some(name.as_str()),
        Expr::SelfRef => Some("self"),
        _ => None,
    };
    if let Some(var_name) = var_name {
        if let Some(struct_name) = body.ctx.var_struct_type.get(var_name).cloned() {
            if let Some(layout) = body.ctx.struct_layouts.get(&struct_name) {
                if let Some(idx) = layout.field_index(field) {
                    let obj = if let Some(var) = body.ctx.get_var(var_name) {
                        body.ir.raw().ins().stack_load(var.cranelift_type, var.slot, 0)
                    } else { return; };
                    let val = emit_expr(body, value);
                    let idx_val = body.ir.const_i64(idx as i64);
                    emit_struct_set(body.ir, obj, idx_val, val, &mut body.ctx);
                }
            }
        }
    }
}

fn emit_let_result(
    body: &mut Body,
    name: &str,
    err_name: &str,
    value: &Expr,
) {
    if let Expr::Call { target, args } = value {
        if let Expr::Ident(fn_name) = target.as_ref() {
            if let Some(func_ref) = body.ctx.get_func(fn_name) {
                let func_ref = *func_ref;
                let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(body, a)).collect();
                let results = body.ir.call_multi(func_ref, &arg_vals);
                if results.len() >= 2 {
                    let val = results[0];
                    let err = results[1];
                    let cl_type = body.ir.value_ir_type(val);
                    let val_slot = body.ir.alloc_var(val);
                    let kind = if body.ir.is_number(val) { RocaType::Number } else { RocaType::Unknown };
                    body.ctx.set_var_kind(name.to_string(), val_slot.0, cl_type, kind);
                    let err_slot = body.ir.alloc_var(err);
                    body.ctx.set_var_kind(err_name.to_string(), err_slot.0, cranelift_codegen::ir::types::I8, RocaType::Bool);
                } else if !results.is_empty() {
                    let val = results[0];
                    let cl_type = body.ir.value_ir_type(val);
                    let val_slot = body.ir.alloc_var(val);
                    body.ctx.set_var(name.to_string(), val_slot.0, cl_type);
                    let zero = body.ir.const_bool(false);
                    let err_slot = body.ir.alloc_var(zero);
                    body.ctx.set_var_kind(err_name.to_string(), err_slot.0, cranelift_codegen::ir::types::I8, RocaType::Bool);
                }
            }
        }
    }
}

fn build_wait_fn_array(
    body: &mut Body,
    exprs: &[roca::Expr],
) -> (Value, Value) {
    let arr = if let Some(&arr_new) = body.ctx.get_func("__array_new") {
        body.ir.call(arr_new, &[])
    } else {
        body.null()
    };
    for expr in exprs {
        let name = format!("__wait_{}", super::compile::wait_expr_hash(expr));
        if let Some(&func_ref) = body.ctx.get_func(&name) {
            let ptr = body.ir.func_addr(func_ref);
            if let Some(&push) = body.ctx.get_func("__array_push_str") {
                body.ir.call_void(push, &[arr, ptr]);
            }
        }
    }
    let count = body.ir.const_i64(exprs.len() as i64);
    (arr, count)
}

fn emit_wait(
    body: &mut Body,
    names: &[String],
    failed_name: &str,
    kind: &roca::WaitKind,
) {
    match kind {
        roca::WaitKind::Single(expr) => {
            let val = emit_expr(body, expr);
            let cl_type = body.ir.value_ir_type(val);
            if !names.is_empty() {
                let slot = body.ir.alloc_var(val);
                let kind = infer_kind(expr, &body.ctx);
                body.ctx.set_var_kind(names[0].clone(), slot.0, cl_type, kind);
            }
            let false_val = body.ir.const_bool(false);
            let err_slot = body.ir.alloc_var(false_val);
            body.ctx.set_var_kind(failed_name.to_string(), err_slot.0, cranelift_codegen::ir::types::I8, RocaType::Bool);
        }
        roca::WaitKind::All(exprs) => {
            let (arr, count) = build_wait_fn_array(body, exprs);
            if let Some(&wait_all) = body.ctx.get_func("__wait_all") {
                let results = body.ir.call(wait_all, &[arr, count]);
                for (i, name) in names.iter().enumerate() {
                    if let Some(&get) = body.ctx.get_func("__array_get_f64") {
                        let idx = body.ir.const_i64(i as i64);
                        let val = body.ir.call(get, &[results, idx]);
                        let slot = body.ir.alloc_var(val);
                        body.ctx.set_var_kind(name.clone(), slot.0, RocaType::Number.to_cranelift(), RocaType::Number);
                    }
                }
            }
            let false_val = body.ir.const_bool(false);
            let err_slot = body.ir.alloc_var(false_val);
            body.ctx.set_var_kind(failed_name.to_string(), err_slot.0, cranelift_codegen::ir::types::I8, RocaType::Bool);
        }
        roca::WaitKind::First(exprs) => {
            let (arr, count) = build_wait_fn_array(body, exprs);
            if let Some(&wait_first) = body.ctx.get_func("__wait_first") {
                let val = body.ir.call(wait_first, &[arr, count]);
                if !names.is_empty() {
                    let slot = body.ir.alloc_var(val);
                    body.ctx.set_var_kind(names[0].clone(), slot.0, RocaType::Number.to_cranelift(), RocaType::Number);
                }
            }
            let false_val = body.ir.const_bool(false);
            let err_slot = body.ir.alloc_var(false_val);
            body.ctx.set_var_kind(failed_name.to_string(), err_slot.0, cranelift_codegen::ir::types::I8, RocaType::Bool);
        }
    }
}
