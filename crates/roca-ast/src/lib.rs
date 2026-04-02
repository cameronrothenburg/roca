//! Abstract syntax tree definitions for the Roca language.
//!
//! This is a leaf crate with no internal dependencies. Nearly every other
//! compiler crate depends on `roca-ast` — the parser produces it, and the
//! checker, JS emitter, native backend, and LSP all consume it.
//!
//! # Key types
//!
//! - [`SourceFile`] / [`Item`] — top-level program structure (functions,
//!   structs, contracts, imports, enums, satisfies, extern declarations).
//! - [`Expr`], [`Stmt`] — expression and statement nodes.
//! - [`TypeRef`] — source-level type references (resolved later by the checker).
//! - [`CrashBlock`] / [`CrashHandler`] — error-recovery strategy trees.
//! - [`TestBlock`] / [`TestCase`] — inline proof-test declarations.
//! - [`ErrDecl`] — named error declarations on fallible functions.

pub mod types;
pub mod expr;
pub mod stmt;
pub mod err;
pub mod crash;
pub mod test_block;
pub mod nodes;
pub mod constants;

pub use nodes::*;
pub use types::TypeRef;
pub use expr::{Expr, BinOp, MatchArm, MatchPattern, StringPart, expr_to_dotted_name, call_to_name};
pub use stmt::{Stmt, WaitKind, collect_returned_error_names};
pub use err::ErrDecl;
pub use crash::{CrashBlock, CrashHandler, CrashHandlerKind, CrashArm, CrashChain, CrashStep};
pub use test_block::{TestBlock, TestCase};
