//! roca-parse — tokenizer and parser for the Roca language.
//!
//! Takes source text, produces `roca_lang::SourceFile`.

mod tokenizer;
mod parser;

pub use tokenizer::tokenize;
pub use parser::{parse, parse_project};

#[cfg(test)]
mod tests;
