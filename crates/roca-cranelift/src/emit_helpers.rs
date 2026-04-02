//! Value type inference, scope cleanup, and shared emit utilities.

use std::sync::LazyLock;
use cranelift_codegen::ir::{self, FuncRef, Value};

use roca_ast::{Expr, BinOp};
use roca_types::RocaType;
use crate::context::EmitCtx;
use crate::cranelift_type::{CleanupRegistry, emit_cleanup};
use crate::builder::IrBuilder;

static CLEANUP_REGISTRY: LazyLock<CleanupRegistry> = LazyLock::new(CleanupRegistry::new);

/// Infer the RocaType of an expression from its AST structure and context.
pub fn infer_kind(expr: &Expr, ctx: &EmitCtx) -> RocaType {
    match expr {
        Expr::Number(_) => RocaType::Number,
        Expr::Bool(_) => RocaType::Bool,
        Expr::String(_) | Expr::StringInterp(_) => RocaType::String,
        Expr::Array(_) => RocaType::Array(Box::new(RocaType::Unknown)),
        Expr::BinOp { op, left, .. } => match op {
            BinOp::Add => {
                let left_kind = infer_kind(left, ctx);
                if left_kind == RocaType::String { RocaType::String } else { RocaType::Number }
            }
            BinOp::Sub | BinOp::Mul | BinOp::Div => RocaType::Number,
            _ => RocaType::Unknown,
        },
        Expr::Call { target, .. } => {
            if let Expr::Ident(name) = target.as_ref() {
                if let Some(kind) = ctx.func_return_kinds.get(name) {
                    return kind.clone();
                }
            }
            if let Expr::FieldAccess { target: obj, field } = target.as_ref() {
                if let Expr::Ident(name) = obj.as_ref() {
                    if ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&field.to_string())) {
                        return RocaType::Enum(name.clone());
                    }
                    match (name.as_str(), field.as_str()) {
                        ("JSON", "parse") | ("JSON", "get") => return RocaType::Struct("Json".into()),
                        ("JSON", "getArray") => return RocaType::Struct("JsonArray".into()),
                        ("Url", "parse") => return RocaType::Struct("Url".into()),
                        ("Http", "get") | ("Http", "post") | ("Http", "put")
                        | ("Http", "patch") | ("Http", "delete") => return RocaType::Struct("HttpResponse".into()),
                        ("Http", "json") => return RocaType::Struct("Json".into()),
                        _ => {}
                    }
                }
            }
            if let Expr::FieldAccess { field, .. } = target.as_ref() {
                return match field.as_str() {
                    "map" | "filter" | "split" => RocaType::Array(Box::new(RocaType::Unknown)),
                    "trim" | "toUpperCase" | "toLowerCase" | "slice"
                    | "charAt" | "join" | "toString" | "concat" => RocaType::String,
                    "indexOf" | "charCodeAt" | "length" | "len" => RocaType::Number,
                    "includes" | "startsWith" | "endsWith" => RocaType::Bool,
                    "push" | "pop" => RocaType::Unknown,
                    _ => RocaType::Unknown,
                };
            }
            RocaType::Unknown
        }
        Expr::StructLit { name, .. } => RocaType::Struct(name.clone()),
        Expr::EnumVariant { enum_name, .. } => RocaType::Enum(enum_name.clone()),
        Expr::Match { arms, .. } => {
            for arm in arms {
                if arm.pattern.is_some() {
                    return infer_kind(&arm.value, ctx);
                }
            }
            RocaType::Unknown
        }
        Expr::Ident(name) => ctx.get_var(name).map(|v| v.kind.clone()).unwrap_or(RocaType::Unknown),
        Expr::FieldAccess { target, field } => {
            if let Expr::Ident(name) = target.as_ref() {
                if ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&field.to_string())) {
                    return RocaType::Enum(name.clone());
                }
            }
            RocaType::Unknown
        }
        Expr::Null => RocaType::Unknown,
        _ => RocaType::Unknown,
    }
}

pub struct FreeRefs {
    pub rc_release: Option<FuncRef>,
    pub free_array: Option<FuncRef>,
    pub free_struct: Option<FuncRef>,
    pub map_free: Option<FuncRef>,
    pub box_free: Option<FuncRef>,
}

impl FreeRefs {
    pub fn from_ctx(ctx: &EmitCtx) -> Self {
        Self {
            rc_release: ctx.func_refs.get("__rc_release").copied(),
            free_array: ctx.func_refs.get("__free_array").copied(),
            free_struct: ctx.func_refs.get("__free_struct").copied(),
            map_free: ctx.func_refs.get("__map_free").copied(),
            box_free: ctx.func_refs.get("__box_free").copied(),
        }
    }
}

/// Release all live heap variables except `skip_name` (the return value).
pub fn emit_scope_cleanup(ir: &mut IrBuilder, ctx: &EmitCtx, skip_name: Option<&str>) {
    let refs = FreeRefs::from_ctx(ctx);
    let registry = &*CLEANUP_REGISTRY;
    for var_name in &ctx.live_heap_vars {
        if skip_name == Some(var_name.as_str()) { continue; }
        if let Some(var) = ctx.vars.get(var_name) {
            if !var.is_heap { continue; }
            let strategy = registry.strategy_for(&var.kind);
            emit_cleanup(ir.b, var.slot, strategy, &refs);
        }
    }
}

/// Emit free for a specific variable by its kind.
pub fn emit_free_by_kind(
    ir: &mut IrBuilder,
    slot: ir::StackSlot,
    _cl_type: ir::Type,
    kind: RocaType,
    refs: &FreeRefs,
) {
    let registry = &*CLEANUP_REGISTRY;
    let strategy = registry.strategy_for(&kind);
    emit_cleanup(ir.b, slot, strategy, refs);
}

/// Release only the loop-body locals (vars declared after loop_heap_base).
pub fn emit_loop_body_cleanup(ir: &mut IrBuilder, ctx: &EmitCtx) {
    let refs = FreeRefs::from_ctx(ctx);
    let registry = &*CLEANUP_REGISTRY;
    for var_name in ctx.live_heap_vars.iter().skip(ctx.loop_heap_base) {
        if let Some(var) = ctx.vars.get(var_name) {
            if !var.is_heap { continue; }
            let strategy = registry.strategy_for(&var.kind);
            emit_cleanup(ir.b, var.slot, strategy, &refs);
        }
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

pub fn emit_length(ir: &mut IrBuilder, obj: Value, kind: RocaType, ctx: &mut EmitCtx) -> Value {
    let is_array = matches!(kind, RocaType::Array(_));
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
