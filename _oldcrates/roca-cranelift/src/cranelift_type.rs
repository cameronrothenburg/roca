//! CraneliftType — extension trait that maps language types to Cranelift IR types.

use cranelift_codegen::ir::{self, types, Value, InstBuilder};
use cranelift_frontend::FunctionBuilder;
use roca_types::RocaType;

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
