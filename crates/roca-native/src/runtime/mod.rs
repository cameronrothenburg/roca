//! Runtime bridge — re-exports from roca-runtime and roca-cranelift.

// Internal: full runtime re-export for tests and emit code within this crate
pub(crate) use roca_runtime::*;
pub(crate) use roca_cranelift::{RuntimeFuncs, register_symbols, declare_runtime};
pub(crate) use roca_cranelift::{reset_constraint_violated, constraint_violated};

// Public: only what roca-cli actually needs
pub use roca_cranelift::{MEM, MemTracker};
