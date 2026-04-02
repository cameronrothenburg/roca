//! Recursive-descent parser that tokenizes Roca source code and produces an AST.
//!
//! Depends on [`roca_ast`] (for node types) and [`roca_errors`] (for
//! `ParseError`). Consumed by `roca-check`, `roca-resolve`, `roca-lsp`, and
//! the CLI.
//!
//! # Key exports
//!
//! - [`parse()`] — parse a source string into a [`roca_ast::SourceFile`],
//!   panicking on syntax errors (used in tests and the build pipeline).
//! - [`try_parse()`] — fallible variant that returns `Result<SourceFile, ParseError>`
//!   (used by the LSP to avoid crashing on incomplete input).
//! - [`tokenize()`] — low-level tokenizer, exposed for tooling.
//!
//! # Example
//!
//! ```
//! let file = roca_parse::parse("pub fn id(x: Number) -> Number { return x test { self(1) == 1 } }");
//! assert_eq!(file.items.len(), 1);
//! ```

pub mod tokenizer;
pub mod string_interp;
pub mod expr;
pub mod contract;
pub mod struct_def;
pub mod satisfies;
pub mod function;
pub mod crash;
pub mod test_block;
pub mod parser;

pub use tokenizer::tokenize;
pub use parser::{parse, try_parse};
