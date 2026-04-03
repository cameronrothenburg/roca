//! Module resolution and cross-file contract registry for the Roca compiler.
//!
//! Depends on [`roca_ast`] and [`roca_parse`]. Consumed by `roca-check` (to
//! resolve imported function signatures) and `roca-cli` (to build the full
//! project registry before checking).
//!
//! # Key exports
//!
//! - [`ContractRegistry`] — collects all contracts, structs, and extern
//!   declarations from a [`roca_ast::SourceFile`] into a queryable registry.
//! - [`find_imported_fn()`] — given a function name and the current file,
//!   follows `import` statements to locate the function's signature in
//!   neighbouring `.roca` files.
//! - [`ResolvedFn`] — lightweight summary of a resolved function's params,
//!   error declarations, and fallibility.

pub mod registry;
pub mod resolve;

pub use resolve::*;
pub use registry::ContractRegistry;
