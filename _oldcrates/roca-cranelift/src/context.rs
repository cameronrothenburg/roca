//! Core types and emission context for native code generation.

use std::collections::HashMap;
use cranelift_codegen::ir::{self, types, StackSlot, Type, FuncRef};

use roca_types::RocaType;

/// Tracks compiled functions for cross-function references
pub struct CompiledFuncs {
    pub(crate) funcs: HashMap<String, cranelift_module::FuncId>,
}

impl CompiledFuncs {
    pub fn new() -> Self { Self { funcs: HashMap::new() } }

    /// Check if a function has been compiled.
    pub fn has(&self, name: &str) -> bool {
        self.funcs.contains_key(name)
    }
}

#[derive(Clone)]
pub struct VarInfo {
    pub(crate) slot: StackSlot,
    pub(crate) cranelift_type: Type,
    pub(crate) kind: RocaType,
    pub(crate) is_heap: bool,
}

/// Tracks struct field layouts for field access by index and type.
#[derive(Clone)]
pub struct StructLayout {
    pub(crate) fields: Vec<(std::string::String, RocaType)>,
}

impl StructLayout {
    /// Create a new struct layout from field definitions.
    pub fn new(fields: Vec<(String, RocaType)>) -> Self {
        Self { fields }
    }
}

impl StructLayout {
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|(f, _)| f == name)
    }

    pub fn field_kind(&self, name: &str) -> RocaType {
        self.fields.iter().find(|(f, _)| f == name).map(|(_, k)| k.clone()).unwrap_or(RocaType::Unknown)
    }
}

/// Everything needed during emission — avoids parameter sprawl.
/// Contains only generic compilation state. Language-specific metadata
/// (crash handlers, enum variants, etc.) belongs in the consuming crate.
pub struct EmitCtx {
    pub(crate) vars: HashMap<String, VarInfo>,
    pub(crate) func_refs: HashMap<String, FuncRef>,
    pub(crate) returns_err: bool,
    pub(crate) return_type: Type,
    pub(crate) struct_layouts: HashMap<String, StructLayout>,
    pub(crate) var_struct_type: HashMap<String, String>,
    pub(crate) live_heap_vars: Vec<String>,
    pub(crate) loop_heap_base: usize,
    pub(crate) loop_exit: Option<ir::Block>,
    pub(crate) loop_header: Option<ir::Block>,
    /// Set by struct_lit/enum_variant, consumed by const_var/let_var to auto-register var_struct_type
    pub(crate) pending_struct_type: Option<String>,
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
