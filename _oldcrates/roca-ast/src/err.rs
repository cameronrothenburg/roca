//! Error declaration AST node — named errors defined in contracts and structs.

/// Named error declaration in a contract or struct contract block
/// e.g. `err timeout = "request timed out"`
#[derive(Debug, Clone, PartialEq)]
pub struct ErrDecl {
    pub name: String,
    pub message: String,
}
