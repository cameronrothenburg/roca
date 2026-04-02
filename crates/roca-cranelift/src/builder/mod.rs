//! High-level Cranelift builder API for the Roca compiler.
//! Hides raw Cranelift types behind Roca-level abstractions.

mod ir;
mod compiler;

pub use ir::{IrBuilder, VarSlot};
pub use compiler::{FunctionCompiler, FunctionSpec, ParamSpec};
