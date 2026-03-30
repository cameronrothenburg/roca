//! Check contexts passed to rules — file, item, function, statement, and expression scopes.

use std::collections::HashMap;
use crate::ast::*;
use super::registry::ContractRegistry;

/// Variable name → type name
pub type Scope = HashMap<String, String>;

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
