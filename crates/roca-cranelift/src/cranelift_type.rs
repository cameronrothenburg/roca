//! CraneliftType — extension trait that adds Cranelift IR behavior to RocaType.
//! This is the bridge between the shared type system and the JIT backend.

use cranelift_codegen::ir::{self, types, Value, InstBuilder};
use cranelift_frontend::FunctionBuilder;
use roca_types::RocaType;

use crate::helpers::{call_void, load_slot};
use crate::emit_helpers::FreeRefs;

/// Extension trait: Cranelift-specific behavior for Roca types.
pub trait CraneliftType {
    /// Map to a Cranelift IR type (F64, I8, or I64).
    fn to_cranelift(&self) -> ir::Type;

    /// Emit cleanup code for a heap-managed value at the given stack slot.
    /// No-op for stack types.
    fn emit_free(&self, b: &mut FunctionBuilder, slot: ir::StackSlot, refs: &FreeRefs);

    /// Produce a default/zero value for this type.
    fn default_value(&self, b: &mut FunctionBuilder) -> Value;
}

impl CraneliftType for RocaType {
    fn to_cranelift(&self) -> ir::Type {
        match self {
            RocaType::Number => types::F64,
            RocaType::Bool | RocaType::Void => types::I8,
            // Everything else is a pointer (I64)
            RocaType::String
            | RocaType::Array(_)
            | RocaType::Map(_, _)
            | RocaType::Struct(_)
            | RocaType::Enum(_)
            | RocaType::Optional(_)
            | RocaType::Fn(_, _)
            | RocaType::Json
            | RocaType::Url
            | RocaType::HttpResponse
            | RocaType::JsonArray
            | RocaType::Unknown => types::I64,
        }
    }

    fn emit_free(&self, b: &mut FunctionBuilder, slot: ir::StackSlot, refs: &FreeRefs) {
        if !self.is_heap() {
            return;
        }
        let ptr = load_slot(b, slot, self.to_cranelift());
        match self {
            RocaType::String => {
                if let Some(f) = refs.rc_release { call_void(b, f, &[ptr]); }
            }
            RocaType::Array(_) => {
                if let Some(f) = refs.free_array { call_void(b, f, &[ptr]); }
            }
            RocaType::JsonArray => {
                if let Some(f) = refs.free_json_array { call_void(b, f, &[ptr]); }
            }
            RocaType::Struct(_) | RocaType::Map(_, _) => {
                if let Some(f) = refs.free_struct {
                    let zero = b.ins().iconst(types::I64, 0);
                    call_void(b, f, &[ptr, zero]);
                }
            }
            RocaType::Enum(_) => {
                if let Some(f) = refs.free_struct {
                    let one = b.ins().iconst(types::I64, 1);
                    call_void(b, f, &[ptr, one]);
                }
            }
            RocaType::Json | RocaType::Url | RocaType::HttpResponse => {
                if let Some(f) = refs.box_free { call_void(b, f, &[ptr]); }
            }
            _ => {}
        }
    }

    fn default_value(&self, b: &mut FunctionBuilder) -> Value {
        match self {
            RocaType::Number => b.ins().f64const(0.0),
            _ => b.ins().iconst(self.to_cranelift(), 0),
        }
    }
}
