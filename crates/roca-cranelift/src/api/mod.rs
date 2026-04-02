//! Roca language-level builder API.
//! Mirrors Roca constructs directly — Function, Struct, Body, if/else, const/let.
//! All IR generation, memory management, and cleanup is internal.

mod body;
mod function;

pub use body::{Body, ConstRef, MutRef, VarRef, StringPart, MatchArm, MatchArmLazy, LazyArmKind};
pub use crate::builder::VarSlot;
pub use function::{Function, Method, Struct, Satisfies, RocaEnum, ExternFn, ExternContract};

// Re-export Value — the opaque handle callers thread through
pub use cranelift_codegen::ir::Value;
