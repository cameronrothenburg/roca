//! Cranelift IR toolkit for the Roca compiler.
//! Provides type mapping, IR helpers, emit context, memory management,
//! and the runtime function registry.

pub(crate) mod types;
pub(crate) mod helpers;
pub mod context;
pub(crate) mod emit_helpers;
pub mod registry;
pub mod cranelift_type;
pub(crate) mod builder;
pub mod api;

// Public API
pub use api::{Body, Function, Method, Struct, Satisfies, RocaEnum, ExternFn, ExternContract};
pub use api::{ConstRef, MutRef, VarRef, StringPart, MatchArm, Value};
pub use context::{CompiledFuncs, EmitCtx};
pub use registry::{RuntimeFuncs, register_symbols, declare_runtime};
pub use cranelift_type::CraneliftType;

// Re-export memory management from roca-runtime
pub use roca_runtime::{MEM, MemTracker, reset_constraint_violated, constraint_violated};
