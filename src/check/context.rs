//! Check contexts passed to rules — file, item, function, statement, and expression scopes.

use std::collections::HashMap;
use crate::ast::*;
use super::registry::ContractRegistry;

/// Variable ownership info — is_const/is_moved will be read once
/// ownership checks migrate from standalone sets to walker scope.
#[derive(Clone, Debug)]
pub struct VarInfo {
    pub type_name: String,
    #[allow(dead_code)]
    pub is_const: bool,
    #[allow(dead_code)]
    pub is_moved: bool,
}

impl VarInfo {
    pub fn new_const(type_name: String) -> Self {
        Self { type_name, is_const: true, is_moved: false }
    }
    pub fn new_let(type_name: String) -> Self {
        Self { type_name, is_const: false, is_moved: false }
    }
}

/// Variable name → ownership info
pub type Scope = HashMap<String, VarInfo>;

/// Context about the function being checked
#[derive(Clone)]
pub struct FnContext<'a> {
    pub def: &'a FnDef,
    pub qualified_name: String,
    pub parent_struct: Option<&'a str>,
}

/// Top-level context available to all rules
pub struct CheckContext<'a> {
    pub file: &'a SourceFile,
    pub registry: &'a ContractRegistry,
    /// Directory the source file lives in — used for resolving relative imports
    pub source_dir: Option<std::path::PathBuf>,
}

/// Context at item level
pub struct ItemContext<'a> {
    pub check: &'a CheckContext<'a>,
    pub item: &'a Item,
}

/// Context at function level
pub struct FnCheckContext<'a> {
    pub check: &'a CheckContext<'a>,
    pub func: FnContext<'a>,
}

/// Context at statement level — with full scope
#[allow(dead_code)]
pub struct StmtContext<'a> {
    pub check: &'a CheckContext<'a>,
    pub func: &'a FnContext<'a>,
    pub scope: &'a Scope,
    pub stmt: &'a Stmt,
}

/// Context at expression level — with full scope
pub struct ExprContext<'a> {
    pub check: &'a CheckContext<'a>,
    pub func: &'a FnContext<'a>,
    pub scope: &'a Scope,
    pub expr: &'a Expr,
}
