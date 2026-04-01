//! High-level Cranelift builder API for the Roca compiler.
//! Hides raw Cranelift types behind Roca-level abstractions.

mod ir;
mod compiler;

pub use ir::{IrBuilder, BlockId, VarSlot};
pub use compiler::{FunctionCompiler, FunctionSpec, ParamSpec};

// Re-export Value and FuncRef — these are unavoidable SSA handles
pub use cranelift_codegen::ir::{Value, FuncRef};
