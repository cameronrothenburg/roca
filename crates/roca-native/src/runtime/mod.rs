//! Runtime bridge — re-exports from roca-runtime and roca-cranelift.

pub use roca_runtime::*;
pub use roca_cranelift::{RuntimeFuncs, register_symbols, declare_runtime};
pub use roca_cranelift::{MEM, MemTracker, reset_constraint_violated, constraint_violated};
