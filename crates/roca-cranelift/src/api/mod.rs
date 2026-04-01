//! Roca language-level builder API.
//! Mirrors Roca constructs directly — Function, Struct, Body, if/else, const/let.
//! All IR generation, memory management, and cleanup is internal.

mod body;
mod function;

pub use body::{Body, ConstRef, MutRef, VarRef, StringPart};
pub use function::Function;

// Re-export Value so callers can thread expression results
pub use cranelift_codegen::ir::Value;
