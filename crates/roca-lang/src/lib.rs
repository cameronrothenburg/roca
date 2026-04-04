//! roca-lang — Shared IR for the Roca language.
//!
//! 26 node types at source-language semantic level. Both backends (JS and native)
//! read this same representation. No memory operations — ownership is tracked by
//! the checker, not encoded in the tree.
//!
//! # Key types
//!
//! - [`SourceFile`] — top-level: a list of [`Item`]s (functions, structs, enums, imports)
//! - [`Expr`] — typed expression: `{ kind: ExprKind, ty: Type }`
//! - [`ExprKind`] — the 18 expression variants (Lit, Ident, BinOp, Call, etc.)
//! - [`Stmt`] — 12 statement variants (Let, Var, Assign, Return, If, Loop, etc.)
//! - [`Type`] — Int, Float, String, Bool, Unit, Named, Array, Fn, Optional
//! - [`Own`] — `O` (owned/consumed) or `B` (borrowed/read-only) on parameters
//!
//! # Design
//!
//! The AST IS the IR. One level, both backends. No intermediate representation.
//! The checker annotates `Expr.ty` during its walk. The compiler reads it.

pub mod ast;
pub use ast::*;

#[cfg(test)]
mod tests;
