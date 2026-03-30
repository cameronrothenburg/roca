//! Parser for Roca source code — tokenizes and builds an AST.

pub mod tokenizer;
pub mod expr;
pub mod contract;
pub mod struct_def;
pub mod satisfies;
pub mod function;
pub mod crash;
pub mod test_block;
pub mod mock;
pub mod parser;

pub use tokenizer::tokenize;
pub use parser::{parse, try_parse};
