//! Roca compiler library — re-exports all workspace crates for library consumers.

pub use roca_ast as ast;
pub use roca_errors as errors;
pub use roca_parse as parse;
pub use roca_resolve as resolve;
pub use roca_check as check;
pub use roca_emit as emit;
pub use roca_native as native;
pub use roca_lsp as lsp;
pub use roca_cli as cli;
