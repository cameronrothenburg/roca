//! Core types and emission context for native code generation.

use std::collections::HashMap;
use cranelift_codegen::ir::{self, types, StackSlot, Type, FuncRef};

use crate::ast::{self as roca, crash::CrashHandlerKind};

/// Tracks compiled functions for cross-function references
pub struct CompiledFuncs {
    pub funcs: HashMap<String, cranelift_module::FuncId>,
}

impl CompiledFuncs {
    pub fn new() -> Self { Self { funcs: HashMap::new() } }
}

#[derive(Clone)]
pub struct VarInfo {
    pub slot: StackSlot,
    pub cranelift_type: Type,
    pub kind: ValKind,
    pub is_heap: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ValKind {
    Number,
    String,
    Bool,
    Array,
    Struct,
    /// Algebraic enum variant — tagged struct with string tag at slot 0
    EnumVariant,
    /// Box-allocated opaque types — freed by roca_box_free
    Json,
    Url,
    HttpResp,
    Other, // unknown — not freed at scope exit (safety: only free what we can identify)
}

/// Tracks struct field layouts for field access by index and type.
#[derive(Clone)]
pub struct StructLayout {
    pub fields: Vec<(std::string::String, ValKind)>,
}

impl StructLayout {
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|(f, _)| f == name)
    }

    pub fn field_kind(&self, name: &str) -> ValKind {
        self.fields.iter().find(|(f, _)| f == name).map(|(_, k)| *k).unwrap_or(ValKind::Other)
    }
}

/// Everything needed during emission — avoids parameter sprawl
pub struct EmitCtx {
    pub vars: HashMap<String, VarInfo>,
    pub func_refs: HashMap<String, FuncRef>,
    pub returns_err: bool,
    pub return_type: Type,
    pub struct_layouts: HashMap<String, StructLayout>,
    pub var_struct_type: HashMap<String, String>,
    pub crash_handlers: HashMap<String, CrashHandlerKind>,
    /// Function name → return kind (for tracking what kind of value a call produces)
    pub func_return_kinds: HashMap<String, ValKind>,
    /// Enum name → set of variant names (for recognizing Token.Plus as enum construction)
    pub enum_variants: HashMap<String, Vec<String>>,
    /// Struct name → field definitions (for constraint validation)
    pub struct_defs: HashMap<String, Vec<roca::Field>>,
    pub live_heap_vars: Vec<String>,
    pub loop_heap_base: usize,
    pub loop_exit: Option<ir::Block>,
    pub loop_header: Option<ir::Block>,
}

impl EmitCtx {
    pub fn get_var(&self, name: &str) -> Option<&VarInfo> {
        self.vars.get(name)
    }

    pub fn set_var(&mut self, name: String, slot: StackSlot, ty: Type) {
        let is_heap = ty == types::I64;
        let kind = match ty {
            t if t == types::F64 => ValKind::Number,
            t if t == types::I8 => ValKind::Bool,
            _ => ValKind::Other,
        };
        if is_heap && !self.live_heap_vars.contains(&name) {
            self.live_heap_vars.push(name.clone());
        }
        self.vars.insert(name, VarInfo { slot, cranelift_type: ty, kind, is_heap });
    }

    pub fn set_var_kind(&mut self, name: String, slot: StackSlot, ty: Type, kind: ValKind) {
        let is_heap = ty == types::I64;
        if is_heap && !self.live_heap_vars.contains(&name) {
            self.live_heap_vars.push(name.clone());
        }
        self.vars.insert(name, VarInfo { slot, cranelift_type: ty, kind, is_heap });
    }

    pub fn get_func(&self, name: &str) -> Option<&FuncRef> {
        self.func_refs.get(name)
    }
}
