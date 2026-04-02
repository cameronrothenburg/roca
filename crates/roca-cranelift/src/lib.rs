//! Cranelift IR builder API for the Roca compiler — maps Roca constructs to
//! Cranelift IR and manages the runtime function registry.
//!
//! Depends on [`roca_ast`], [`roca_types`], and [`roca_runtime`]. Consumed by
//! `roca-native`, which uses this crate's builder API to compile functions,
//! structs, and methods into Cranelift IR for JIT or AOT execution.
//!
//! # Key exports
//!
//! - **Builder API** ([`api`] module) — [`Function`], [`Method`], [`Struct`],
//!   [`Satisfies`], [`RocaEnum`], [`ExternFn`], [`ExternContract`], [`Body`]
//!   provide a high-level interface for emitting IR without touching raw
//!   Cranelift instructions.
//! - [`EmitCtx`] / [`CompiledFuncs`] / [`StructLayout`] — compilation context
//!   tracking declared functions, struct field layouts, and scope state.
//! - [`RuntimeFuncs`] / [`register_symbols`] / [`declare_runtime`] — registry
//!   that maps `roca_runtime` host functions into Cranelift function references.
//! - [`CraneliftType`] — Roca-to-Cranelift type mapping.

pub(crate) mod types;
pub(crate) mod helpers;
pub(crate) mod context;
pub(crate) mod emit_helpers;
pub(crate) mod registry;
pub(crate) mod cranelift_type;
pub(crate) mod builder;
pub mod api;

// Public API
pub use api::{Body, Function, Method, Struct, Satisfies, RocaEnum, ExternFn, ExternContract};
pub use api::{ConstRef, MutRef, VarRef, StringPart, MatchArm, MatchArmLazy, LazyArmKind, Value};
pub use context::{CompiledFuncs, EmitCtx, StructLayout};
pub use registry::{RuntimeFuncs, register_symbols, declare_runtime};
pub use cranelift_type::CraneliftType;

// Re-export memory management from roca-runtime
pub use roca_runtime::{MEM, MemTracker, reset_constraint_violated, constraint_violated};
