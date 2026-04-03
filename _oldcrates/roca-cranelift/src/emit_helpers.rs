//! Scope cleanup and shared emit utilities.
//! Single-owner model: every heap var gets `__free(ptr)` at cleanup time.

use cranelift_codegen::ir::{self, InstBuilder, Value};

use crate::context::EmitCtx;
use crate::builder::IrBuilder;
use crate::helpers::load_slot;

/// Free a single variable — frees heap fields if struct, then frees the value.
/// Public so assign() can use it for struct field cleanup on reassignment.
pub fn emit_free_var(ir: &mut IrBuilder, ctx: &EmitCtx, var_name: &str) {
    let free_ref = match ctx.get_func("__free") {
        Some(&f) => f,
        None => return,
    };
    emit_free_var_inner(ir, ctx, var_name, free_ref);
}

pub fn emit_free_var_inner(ir: &mut IrBuilder, ctx: &EmitCtx, var_name: &str, free_ref: ir::FuncRef) {
    let var = match ctx.vars.get(var_name) {
        Some(v) if v.is_heap => v,
        _ => return,
    };
    let ptr = load_slot(ir.b, var.slot, ir::types::I64);

    // If this variable is a struct/enum, free its heap fields first
    if let Some(struct_name) = ctx.var_struct_type.get(var_name) {
        if let Some(layout) = ctx.struct_layouts.get(struct_name) {
            let get_ptr = ctx.get_func("__struct_get_ptr").copied();
            if let Some(get_fn) = get_ptr {
                for (i, (_, kind)) in layout.fields.iter().enumerate() {
                    if kind.is_heap() {
                        let idx = ir.b.ins().iconst(ir::types::I64, i as i64);
                        let field_ptr = ir.call(get_fn, &[ptr, idx]);
                        ir.call_void(free_ref, &[field_ptr]);
                    }
                }
            }
        }
    }

    // Free the value itself
    crate::helpers::call_void(ir.b, free_ref, &[ptr]);
}

/// Release all live heap variables except `skip_name` (the return value).
pub fn emit_scope_cleanup(ir: &mut IrBuilder, ctx: &EmitCtx, skip_name: Option<&str>) {
    let free_ref = match ctx.get_func("__free") {
        Some(&f) => f,
        None => return,
    };
    for var_name in &ctx.live_heap_vars {
        if skip_name == Some(var_name.as_str()) { continue; }
        emit_free_var_inner(ir, ctx, var_name, free_ref);
    }
}

/// Release only the loop-body locals (vars declared after loop_heap_base).
pub fn emit_loop_body_cleanup(ir: &mut IrBuilder, ctx: &EmitCtx) {
    let free_ref = match ctx.get_func("__free") {
        Some(&f) => f,
        None => return,
    };
    for var_name in ctx.live_heap_vars.iter().skip(ctx.loop_heap_base) {
        emit_free_var_inner(ir, ctx, var_name, free_ref);
    }
}

pub fn emit_array_push(ir: &mut IrBuilder, arr: Value, val: Value, ctx: &mut EmitCtx) {
    if ir.is_number(val) {
        if let Some(&f) = ctx.get_func("__array_push_f64") { ir.call_void(f, &[arr, val]); }
    } else {
        if let Some(&f) = ctx.get_func("__array_push_str") { ir.call_void(f, &[arr, val]); }
    }
}

pub fn emit_struct_set(ir: &mut IrBuilder, ptr: Value, idx: Value, val: Value, ctx: &mut EmitCtx) {
    if ir.is_number(val) {
        if let Some(&f) = ctx.get_func("__struct_set_f64") { ir.call_void(f, &[ptr, idx, val]); }
    } else {
        if let Some(&f) = ctx.get_func("__struct_set_ptr") { ir.call_void(f, &[ptr, idx, val]); }
    }
}

pub fn emit_length(ir: &mut IrBuilder, obj: Value, kind: roca_types::RocaType, ctx: &mut EmitCtx) -> Value {
    let is_array = matches!(kind, roca_types::RocaType::Array(_));
    let len_func = if is_array {
        ctx.get_func("__array_len").copied()
    } else {
        ctx.get_func("__string_len").copied()
    };
    if let Some(f) = len_func {
        let len = ir.call(f, &[obj]);
        ir.i64_to_f64(len)
    } else {
        ir.const_number(0.0)
    }
}
