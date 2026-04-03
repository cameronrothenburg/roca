//! roca-check — ownership inference and type checking.
//!
//! Walks the AST, tracks ownership state per value (Austral-style state table),
//! and produces diagnostics for violations.
//!
//! Rules are pluggable structs implementing the [`rule::Rule`] trait. The walker
//! owns all state mutations; rules are pure observers called at each check point.

use roca_lang::SourceFile;

pub mod rule;
mod walker;
mod rules;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
}

fn all_rules() -> Vec<Box<dyn rule::Rule>> {
    vec![
        Box::new(rules::ConstOwns),
        Box::new(rules::LetBorrowsFromConst),
        Box::new(rules::BorrowBeforePass),
        Box::new(rules::UseAfterMove),
        Box::new(rules::DeclareIntent),
        Box::new(rules::ReturnOwned),
        Box::new(rules::ContainerCopy),
        Box::new(rules::BranchSymmetry),
        Box::new(rules::LoopConsumption),
        Box::new(rules::ReturnTypeMismatch),
        Box::new(rules::UnknownType),
        Box::new(rules::UnknownField),
    ]
}

/// Check a source file for ownership and type errors.
/// Returns an empty vec if the program is valid.
pub fn check(source: &SourceFile) -> Vec<Diagnostic> {
    let mut rules = all_rules();
    walker::walk(source, &mut rules)
}

#[cfg(test)]
mod tests {
    mod ownership;  // E-OWN-001 through E-OWN-010
    mod types;      // E-TYP, E-STR
    mod acceptance; // full valid programs
}
