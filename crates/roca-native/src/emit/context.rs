//! NativeCtx — Roca-specific compilation state that lives alongside the generic Body.
//!
//! This holds metadata that the AST walker needs to make Roca-specific decisions
//! (crash handlers, enum variants, function return types, struct definitions).
//! The generic Body in roca-cranelift knows nothing about these concepts.

use std::collections::HashMap;
use roca_ast::{self as roca, crash::CrashHandlerKind, Expr, BinOp};
use roca_cranelift::api::Body;
use roca_types::RocaType;

/// Roca-specific compilation context, passed alongside Body to emit functions.
#[derive(Default)]
pub struct NativeCtx {
    /// Function name → crash handler strategy
    pub crash_handlers: HashMap<String, CrashHandlerKind>,
    /// Function name → return RocaType
    pub func_return_kinds: HashMap<String, RocaType>,
    /// Enum name → variant names
    pub enum_variants: HashMap<String, Vec<String>>,
    /// Struct name → field definitions (for constraint validation)
    pub struct_defs: HashMap<String, Vec<roca::Field>>,
}

impl NativeCtx {
    /// Get crash handler for a function name.
    pub fn get_crash_handler(&self, name: &str) -> Option<&CrashHandlerKind> {
        self.crash_handlers.get(name)
    }

    /// Check if a type.variant is a known enum variant.
    pub fn is_enum_variant(&self, type_name: &str, variant: &str) -> bool {
        self.enum_variants.get(type_name)
            .map_or(false, |vs| vs.iter().any(|v| v == variant))
    }

    /// Get struct field definitions.
    pub fn struct_defs(&self, name: &str) -> Option<&[roca::Field]> {
        self.struct_defs.get(name).map(|v| v.as_slice())
    }

    /// Infer the RocaType of an AST expression using Roca-specific knowledge.
    pub fn infer_type(&self, expr: &Expr, body: &Body) -> RocaType {
        match expr {
            Expr::Number(_) => RocaType::Number,
            Expr::Bool(_) => RocaType::Bool,
            Expr::String(_) | Expr::StringInterp(_) => RocaType::String,
            Expr::Array(_) => RocaType::Array(Box::new(RocaType::Unknown)),
            Expr::BinOp { op, left, .. } => match op {
                BinOp::Add => {
                    let left_kind = self.infer_type(left, body);
                    if left_kind == RocaType::String { RocaType::String } else { RocaType::Number }
                }
                BinOp::Sub | BinOp::Mul | BinOp::Div => RocaType::Number,
                _ => RocaType::Unknown,
            },
            Expr::Call { target, .. } => {
                if let Expr::Ident(name) = target.as_ref() {
                    if let Some(kind) = self.func_return_kinds.get(name) {
                        return kind.clone();
                    }
                }
                if let Expr::FieldAccess { target: obj, field } = target.as_ref() {
                    if let Expr::Ident(name) = obj.as_ref() {
                        if self.is_enum_variant(name, field) {
                            return RocaType::Enum(name.clone());
                        }
                        if let Some(t) = stdlib_static_method_type(name, field) {
                            return t;
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
                        return self.infer_type(&arm.value, body);
                    }
                }
                RocaType::Unknown
            }
            Expr::Ident(name) => body.var_kind(name).unwrap_or(RocaType::Unknown),
            Expr::FieldAccess { target, field } => {
                if let Expr::Ident(name) = target.as_ref() {
                    if self.is_enum_variant(name, field) {
                        return RocaType::Enum(name.clone());
                    }
                }
                RocaType::Unknown
            }
            Expr::Null => RocaType::Unknown,
            _ => RocaType::Unknown,
        }
    }
}

/// Return type of well-known static contract methods (e.g. JSON.parse, Http.get).
/// Add new stdlib entries here rather than inlining them in infer_type.
fn stdlib_static_method_type(contract: &str, method: &str) -> Option<RocaType> {
    match (contract, method) {
        ("JSON", "parse") | ("JSON", "get") => Some(RocaType::Struct("Json".into())),
        ("JSON", "getArray") => Some(RocaType::Struct("JsonArray".into())),
        ("Url", "parse") => Some(RocaType::Struct("Url".into())),
        ("Http", "get") | ("Http", "post") | ("Http", "put")
        | ("Http", "patch") | ("Http", "delete") => Some(RocaType::Struct("HttpResponse".into())),
        ("Http", "json") => Some(RocaType::Struct("Json".into())),
        _ => None,
    }
}
