//! roca-parse — tokenizer, parser, and ownership checker for Roca.
//!
//! Takes source text, produces a checked `SourceFile` or diagnostics.
//! Code that violates ownership rules cannot produce an AST.

mod tokenizer;
mod parser;
pub mod rule;
mod walker;
mod rules;

pub use tokenizer::tokenize;
pub use rule::Diagnostic;

use roca_lang::SourceFile;

/// Result of parsing and checking a source file.
pub struct ParseResult {
    pub ast: SourceFile,
    pub errors: Vec<Diagnostic>,  // blocking — code won't compile
    pub notes: Vec<Diagnostic>,   // informational — code compiles but something implicit happened
}

impl ParseResult {
    pub fn is_ok(&self) -> bool { self.errors.is_empty() }
}

/// Parse and check a single Roca source file.
pub fn parse(source: &str) -> ParseResult {
    let ast = parser::parse(source);
    let diags = walker::walk(&ast, &mut rules::all_rules());
    let mut errors = Vec::new();
    let mut notes = Vec::new();
    for d in diags {
        if d.code == "E-OWN-007" {
            notes.push(d);
        } else {
            errors.push(d);
        }
    }
    ParseResult { ast, errors, notes }
}

/// Parse and check multiple files as a project.
pub fn parse_project(files: &[(&str, &str)]) -> ParseResult {
    let mut all_items = Vec::new();
    let mut all_errors = Vec::new();
    let mut all_notes = Vec::new();
    for (_, src) in files {
        let result = parse(src);
        all_items.extend(result.ast.items);
        all_errors.extend(result.errors);
        all_notes.extend(result.notes);
    }
    ParseResult {
        ast: SourceFile { items: all_items },
        errors: all_errors,
        notes: all_notes,
    }
}

#[cfg(test)]
mod tests;
