//! CraneliftType — extension trait that adds Cranelift IR behavior to RocaType.
//! This is the bridge between the shared type system and the JIT backend.
//!
//! Cleanup is strategy-based: each type has a CleanupStrategy, and named types
//! (Struct) can override via a registered lookup. This keeps the system open
//! for extern contracts — a user's `Redis` type gets the same treatment as `Json`.

use std::collections::HashMap;
use cranelift_codegen::ir::{self, types, Value, InstBuilder};
use cranelift_frontend::FunctionBuilder;
use roca_types::RocaType;

use crate::helpers::{call_void, load_slot};
use crate::emit_helpers::FreeRefs;

/// Cleanup strategy for heap-managed types — Cranelift-specific.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CleanupStrategy {
    RcRelease,
    FreeArray,
    FreeStruct { heap_fields: u32 },
    FreeEnum,
    BoxFree,
    None,
}

/// Default cleanup strategy for a RocaType.
pub fn default_cleanup(ty: &RocaType) -> CleanupStrategy {
    match ty {
        RocaType::String => CleanupStrategy::RcRelease,
        RocaType::Array(_) => CleanupStrategy::FreeArray,
        RocaType::Map(_, _) | RocaType::Struct(_) => CleanupStrategy::FreeStruct { heap_fields: 0 },
        RocaType::Enum(_) => CleanupStrategy::FreeEnum,
        _ => CleanupStrategy::None,
    }
}

/// Cleanup overrides for named types (Json → BoxFree, Url → BoxFree, etc.).
/// Backends register these at startup. User extern contracts can register too.
pub struct CleanupRegistry {
    overrides: HashMap<String, CleanupStrategy>,
}

impl CleanupRegistry {
    pub fn new() -> Self {
        let mut overrides = HashMap::new();
        // Built-in runtime types that need box-free instead of struct-free
        overrides.insert("Json".into(), CleanupStrategy::BoxFree);
        overrides.insert("Url".into(), CleanupStrategy::BoxFree);
        overrides.insert("HttpResponse".into(), CleanupStrategy::BoxFree);
        overrides.insert("JsonArray".into(), CleanupStrategy::FreeArray); // array of boxed JSON
        Self { overrides }
    }

    /// Register a cleanup strategy for a named type (e.g., extern contract).
    #[allow(dead_code)]
    pub fn register(&mut self, name: impl Into<String>, strategy: CleanupStrategy) {
        self.overrides.insert(name.into(), strategy);
    }

    /// Look up the cleanup strategy for a type, checking overrides first.
    pub fn strategy_for(&self, ty: &RocaType) -> CleanupStrategy {
        if let RocaType::Struct(name) | RocaType::Enum(name) = ty {
            if let Some(&strategy) = self.overrides.get(name.as_str()) {
                return strategy;
            }
        }
        default_cleanup(ty)
    }
}

/// Extension trait: Cranelift-specific behavior for Roca types.
pub trait CraneliftType {
    /// Map to a Cranelift IR type (F64, I8, or I64).
    fn to_cranelift(&self) -> ir::Type;

    /// Produce a default/zero value for this type.
    fn default_value(&self, b: &mut FunctionBuilder) -> Value;
}

impl CraneliftType for RocaType {
    fn to_cranelift(&self) -> ir::Type {
        match self {
            RocaType::Number => types::F64,
            RocaType::Bool | RocaType::Void => types::I8,
            _ => types::I64,
        }
    }

    fn default_value(&self, b: &mut FunctionBuilder) -> Value {
        match self {
            RocaType::Number => b.ins().f64const(0.0),
            _ => b.ins().iconst(self.to_cranelift(), 0),
        }
    }
}

/// Emit cleanup for a value based on its cleanup strategy.
/// This is the single dispatch point — no more 8-arm ValKind match.
pub fn emit_cleanup(
    b: &mut FunctionBuilder,
    slot: ir::StackSlot,
    strategy: CleanupStrategy,
    refs: &FreeRefs,
) {
    match strategy {
        CleanupStrategy::None => {}
        CleanupStrategy::RcRelease => {
            if let Some(f) = refs.rc_release {
                let ptr = load_slot(b, slot, types::I64);
                call_void(b, f, &[ptr]);
            }
        }
        CleanupStrategy::FreeArray => {
            if let Some(f) = refs.free_array {
                let ptr = load_slot(b, slot, types::I64);
                call_void(b, f, &[ptr]);
            }
        }
        CleanupStrategy::FreeStruct { heap_fields } => {
            if let Some(f) = refs.free_struct {
                let ptr = load_slot(b, slot, types::I64);
                let n = b.ins().iconst(types::I64, heap_fields as i64);
                call_void(b, f, &[ptr, n]);
            }
        }
        CleanupStrategy::FreeEnum => {
            if let Some(f) = refs.free_struct {
                let ptr = load_slot(b, slot, types::I64);
                let one = b.ins().iconst(types::I64, 1);
                call_void(b, f, &[ptr, one]);
            }
        }
        CleanupStrategy::BoxFree => {
            if let Some(f) = refs.box_free {
                let ptr = load_slot(b, slot, types::I64);
                call_void(b, f, &[ptr]);
            }
        }
    }
}
