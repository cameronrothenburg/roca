//! Value kind inference, scope cleanup, and shared emit utilities.

use cranelift_codegen::ir::{self, types, Value, FuncRef, InstBuilder};
use cranelift_frontend::FunctionBuilder;

use crate::ast::{self as roca, Expr, BinOp};
use crate::native::helpers::{call_rt, call_void, load_slot};
use super::context::{EmitCtx, ValKind};

pub fn type_ref_to_kind(ty: &roca::TypeRef) -> ValKind {
    match ty {
        roca::TypeRef::Number => ValKind::Number,
        roca::TypeRef::Bool => ValKind::Bool,
        roca::TypeRef::String => ValKind::String,
        roca::TypeRef::Ok => ValKind::Bool,
        roca::TypeRef::Named(_) => ValKind::Struct,
        roca::TypeRef::Generic(name, _) => match name.as_str() {
            "Array" => ValKind::Array,
            "Map" => ValKind::Struct,
            _ => ValKind::Struct,
        },
        roca::TypeRef::Nullable(_) => ValKind::Other,
        roca::TypeRef::Fn(_, _) => ValKind::Other,
    }
}

pub fn infer_kind(expr: &Expr, ctx: &EmitCtx) -> ValKind {
    match expr {
        Expr::Number(_) => ValKind::Number,
        Expr::Bool(_) => ValKind::Bool,
        Expr::String(_) | Expr::StringInterp(_) => ValKind::String,
        Expr::Array(_) => ValKind::Array,
        Expr::BinOp { op, left, .. } => match op {
            BinOp::Add => {
                let left_kind = infer_kind(left, ctx);
                if left_kind == ValKind::String { ValKind::String } else { ValKind::Number }
            }
            BinOp::Sub | BinOp::Mul | BinOp::Div => ValKind::Number,
            _ => ValKind::Other,
        },
        Expr::Call { target, .. } => {
            if let Expr::Ident(name) = target.as_ref() {
                if let Some(&kind) = ctx.func_return_kinds.get(name) {
                    return kind;
                }
            }
            if let Expr::FieldAccess { target: obj, field } = target.as_ref() {
                if let Expr::Ident(name) = obj.as_ref() {
                    if ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&field.to_string())) {
                        return ValKind::EnumVariant;
                    }
                    // Stdlib calls returning boxed types
                    match (name.as_str(), field.as_str()) {
                        ("JSON", "parse") | ("JSON", "get") => return ValKind::Json,
                        ("JSON", "getArray") => return ValKind::JsonArray,
                        ("Url", "parse") => return ValKind::Url,
                        ("Http", "get") | ("Http", "post") | ("Http", "put")
                        | ("Http", "patch") | ("Http", "delete") => return ValKind::HttpResp,
                        ("Http", "json") => return ValKind::Json,
                        _ => {}
                    }
                }
            }
            if let Expr::FieldAccess { field, .. } = target.as_ref() {
                return match field.as_str() {
                    "map" | "filter" | "split" => ValKind::Array,
                    "trim" | "toUpperCase" | "toLowerCase" | "slice"
                    | "charAt" | "join" | "toString" | "concat" => ValKind::String,
                    "indexOf" | "charCodeAt" | "length" | "len" => ValKind::Number,
                    "includes" | "startsWith" | "endsWith" => ValKind::Bool,
                    "push" | "pop" => ValKind::Other,
                    _ => ValKind::Other,
                };
            }
            ValKind::Other
        }
        Expr::StructLit { .. } => ValKind::Struct,
        Expr::EnumVariant { .. } => ValKind::EnumVariant,
        Expr::Match { arms, .. } => {
            for arm in arms {
                if arm.pattern.is_some() {
                    return infer_kind(&arm.value, ctx);
                }
            }
            ValKind::Other
        }
        Expr::Ident(name) => ctx.get_var(name).map(|v| v.kind).unwrap_or(ValKind::Other),
        Expr::FieldAccess { target, field } => {
            if let Expr::Ident(name) = target.as_ref() {
                if ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&field.to_string())) {
                    return ValKind::EnumVariant;
                }
            }
            ValKind::Other
        }
        Expr::Null => ValKind::Other,
        _ => ValKind::Other,
    }
}

pub struct FreeRefs {
    pub rc_release: Option<FuncRef>,
    pub free_array: Option<FuncRef>,
    pub free_json_array: Option<FuncRef>,
    pub free_struct: Option<FuncRef>,
    pub box_free: Option<FuncRef>,
}

impl FreeRefs {
    pub fn from_ctx(ctx: &EmitCtx) -> Self {
        Self {
            rc_release: ctx.func_refs.get("__rc_release").copied(),
            free_array: ctx.func_refs.get("__free_array").copied(),
            free_json_array: ctx.func_refs.get("__free_json_array").copied(),
            free_struct: ctx.func_refs.get("__free_struct").copied(),
            box_free: ctx.func_refs.get("__box_free").copied(),
        }
    }
}

/// Release all live heap variables except `skip_name` (the return value).
pub fn emit_scope_cleanup(b: &mut FunctionBuilder, ctx: &EmitCtx, skip_name: Option<&str>) {
    let refs = FreeRefs::from_ctx(ctx);
    for var_name in &ctx.live_heap_vars {
        if skip_name == Some(var_name.as_str()) { continue; }
        if let Some(var) = ctx.vars.get(var_name) {
            if !var.is_heap { continue; }
            emit_free_by_kind(b, var.slot, var.cranelift_type, var.kind, &refs);
        }
    }
}

pub fn emit_free_by_kind(
    b: &mut FunctionBuilder,
    slot: ir::StackSlot,
    cl_type: ir::Type,
    kind: ValKind,
    refs: &FreeRefs,
) {
    let ptr = load_slot(b, slot, cl_type);
    match kind {
        ValKind::String => { if let Some(f) = refs.rc_release { call_void(b, f, &[ptr]); } }
        ValKind::Array => { if let Some(f) = refs.free_array { call_void(b, f, &[ptr]); } }
        ValKind::JsonArray => { if let Some(f) = refs.free_json_array { call_void(b, f, &[ptr]); } }
        ValKind::Struct => {
            if let Some(f) = refs.free_struct {
                let zero = b.ins().iconst(types::I64, 0);
                call_void(b, f, &[ptr, zero]);
            }
        }
        ValKind::EnumVariant => {
            if let Some(f) = refs.free_struct {
                let one = b.ins().iconst(types::I64, 1);
                call_void(b, f, &[ptr, one]);
            }
        }
        ValKind::Json | ValKind::Url | ValKind::HttpResp => {
            if let Some(f) = refs.box_free { call_void(b, f, &[ptr]); }
        }
        _ => {}
    }
}

/// Release only the loop-body locals (vars declared after loop_heap_base).
pub fn emit_loop_body_cleanup(b: &mut FunctionBuilder, ctx: &EmitCtx) {
    let refs = FreeRefs::from_ctx(ctx);
    for var_name in ctx.live_heap_vars.iter().skip(ctx.loop_heap_base) {
        if let Some(var) = ctx.vars.get(var_name) {
            if !var.is_heap { continue; }
            emit_free_by_kind(b, var.slot, var.cranelift_type, var.kind, &refs);
        }
    }
}

pub fn target_kind(expr: &Expr, ctx: &mut EmitCtx) -> ValKind {
    match expr {
        Expr::Ident(name) => ctx.get_var(name).map(|v| v.kind).unwrap_or(ValKind::Other),
        Expr::String(_) | Expr::StringInterp(_) => ValKind::String,
        Expr::Array(_) => ValKind::Array,
        Expr::StructLit { .. } => ValKind::Struct,
        Expr::Number(_) => ValKind::Number,
        _ => ValKind::Other,
    }
}

pub fn first_arg_or_null(b: &mut FunctionBuilder, args: &[Expr], ctx: &mut EmitCtx) -> Value {
    args.first().map(|a| super::expr::emit_expr(b, a, ctx))
        .unwrap_or_else(|| b.ins().iconst(types::I64, 0))
}

pub fn emit_array_push(b: &mut FunctionBuilder, arr: Value, val: Value, ctx: &mut EmitCtx) {
    let ty = b.func.dfg.value_type(val);
    if ty == types::F64 {
        if let Some(&f) = ctx.get_func("__array_push_f64") { call_void(b, f, &[arr, val]); }
    } else {
        if let Some(&f) = ctx.get_func("__array_push_str") { call_void(b, f, &[arr, val]); }
    }
}

pub fn emit_struct_set(b: &mut FunctionBuilder, ptr: Value, idx: Value, val: Value, ctx: &mut EmitCtx) {
    let ty = b.func.dfg.value_type(val);
    if ty == types::F64 {
        if let Some(&f) = ctx.get_func("__struct_set_f64") { call_void(b, f, &[ptr, idx, val]); }
    } else {
        if let Some(&f) = ctx.get_func("__struct_set_ptr") { call_void(b, f, &[ptr, idx, val]); }
    }
}

pub fn emit_length(b: &mut FunctionBuilder, obj: Value, kind: ValKind, ctx: &mut EmitCtx) -> Value {
    let len_func = if kind == ValKind::Array {
        ctx.get_func("__array_len").copied()
    } else {
        ctx.get_func("__string_len").copied()
    };
    if let Some(f) = len_func {
        let len = call_rt(b, f, &[obj]);
        b.ins().fcvt_from_sint(types::F64, len)
    } else {
        b.ins().f64const(0.0)
    }
}
