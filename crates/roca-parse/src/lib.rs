//! roca-parse — tokenizer and parser for the Roca language.
//!
//! Takes source text, produces `roca_lang::SourceFile`.

use roca_lang::SourceFile;

/// Parse a single Roca source file.
pub fn parse(_source: &str) -> SourceFile {
    SourceFile { items: vec![] }
}

/// Parse multiple files as a project. Resolves imports across files.
/// Each entry is (filename, source_text).
pub fn parse_project(_files: &[(&str, &str)]) -> Vec<SourceFile> {
    vec![]
}

#[cfg(test)]
mod tests;
