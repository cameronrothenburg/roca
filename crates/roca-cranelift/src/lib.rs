//! Generic Cranelift IR toolkit for building language compilers.
//!
//! Provides a high-level builder API (Function, Body) that maps language
//! constructs to Cranelift IR — control flow, variables, memory management,
//! pattern matching — without knowing anything about the source language.
//!
//! Depends on [`roca_types`] and [`roca_runtime`]. Consumed by `roca-native`.
//!
//! # Domain Boundary
//!
//! This crate owns **WHEN** memory is freed — the lifecycle:
//! - Scope exit: free all `const` and remaining `let` bindings
//! - Reassignment: free the old value before storing the new one
//! - Temp flush: free unclaimed heap temporaries at statement boundaries
//! - Loop iteration: free loop-local variables before jumping back
//! - Match arms: free arm-local temporaries before jumping to merge
//!
//! It does NOT own **HOW** values are freed — that belongs in `roca-runtime`.
//! Body emits `call __free(ptr)`. Runtime decides the deallocation strategy
//! (tags, layouts, recursive child freeing).
//!
//! This crate does NOT know about:
//! - Roca AST nodes or source parsing
//! - Crash handlers, test runners, property testing (roca-native's domain)
//! - Stdlib function implementations (roca-runtime's domain)
//! - Language-specific orchestration (which functions to compile, ordering)
//!
//! Tests here verify Body/Function API correctness and memory lifecycle
//! (allocs == frees). End-to-end Roca compilation tests belong in roca-native.
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
pub mod lang_type;
pub mod module;
pub mod api;

// Public API
pub use api::{Body, Function, Method, Struct, Satisfies, RocaEnum, ExternFn, ExternContract};
pub use api::{ConstRef, MutRef, VarRef, StringPart, MatchArm, MatchArmLazy, LazyArmKind, Value};
pub use context::{CompiledFuncs, EmitCtx, StructLayout};
pub use registry::{RuntimeFuncs, register_symbols, declare_runtime};
pub use cranelift_type::CraneliftType;
pub use lang_type::LangType;
pub use module::{JitModule, FnDecl, declare_functions};

// Re-export cranelift module types for consuming crates
pub use cranelift_module::{Module, FuncId};

#[cfg(test)]
mod tests_memory;

// Re-export memory management from roca-runtime
pub use roca_runtime::{MEM, MemTracker, reset_constraint_violated, constraint_violated};
