//! Cranelift IR toolkit for the Roca compiler.
//! Provides type mapping, IR helpers, emit context, memory management,
//! and the runtime function registry.

pub mod types;
pub mod helpers;
pub mod context;
pub mod emit_helpers;
pub mod registry;
pub mod cranelift_type;
pub mod builder;

// Re-export key types for convenience
pub use context::{CompiledFuncs, ValKind, VarInfo, StructLayout, EmitCtx};
pub use registry::{RuntimeFuncs, register_symbols, declare_runtime};
pub use types::roca_to_cranelift;

// Re-export the extension trait and cleanup system
pub use cranelift_type::{CraneliftType, CleanupRegistry, emit_cleanup};

// Re-export memory management from roca-runtime
pub use roca_runtime::{MEM, MemTracker, reset_constraint_violated, constraint_violated};
