//! Core types and emission context for native code generation.

use std::collections::HashMap;
use cranelift_codegen::ir::{self, types, StackSlot, Type, FuncRef};

use roca_ast::{self as roca, crash::CrashHandlerKind};
use roca_types::RocaType;
use crate::cranelift_type::CraneliftType;

/// Tracks compiled functions for cross-function references
pub struct CompiledFuncs {
    pub funcs: HashMap<String, cranelift_module::FuncId>,
}

impl CompiledFuncs {
    pub fn new() -> Self { Self { funcs: HashMap::new() } }
}

/// Deprecated — use RocaType directly. Kept as alias during migration.
pub type ValKind = RocaType;

#[derive(Clone)]
pub struct VarInfo {
    pub slot: StackSlot,
    pub cranelift_type: Type,
    pub kind: RocaType,
    pub is_heap: bool,
}

/// Tracks struct field layouts for field access by index and type.
#[derive(Clone)]
pub struct StructLayout {
    pub fields: Vec<(std::string::String, RocaType)>,
}

impl StructLayout {
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|(f, _)| f == name)
    }

    pub fn field_kind(&self, name: &str) -> RocaType {
        self.fields.iter().find(|(f, _)| f == name).map(|(_, k)| k.clone()).unwrap_or(RocaType::Unknown)
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
    /// Function name → return type (for tracking what kind of value a call produces)
    pub func_return_kinds: HashMap<String, RocaType>,
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
            t if t == types::F64 => RocaType::Number,
            t if t == types::I8 => RocaType::Bool,
            _ => RocaType::Unknown,
        };
        if is_heap && !self.live_heap_vars.contains(&name) {
            self.live_heap_vars.push(name.clone());
        }
        self.vars.insert(name, VarInfo { slot, cranelift_type: ty, kind, is_heap });
    }

    pub fn set_var_kind(&mut self, name: String, slot: StackSlot, ty: Type, kind: RocaType) {
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
