//! Rule trait — the interface every checker rule implements.

use crate::errors::RuleError;
use super::context::*;

/// A single lint/check rule.
/// Implement whichever hooks you need — all have default empty implementations.
#[allow(dead_code)]
pub trait Rule {
    fn name(&self) -> &'static str;

    /// Called once per top-level item
    fn check_item(&self, _ctx: &ItemContext) -> Vec<RuleError> { vec![] }

    /// Called once per function body (top-level fn, struct method, satisfies method)
    fn check_function(&self, _ctx: &FnCheckContext) -> Vec<RuleError> { vec![] }

    /// Called for each statement, with scope populated up to this point
    fn check_stmt(&self, _ctx: &StmtContext) -> Vec<RuleError> { vec![] }

    /// Called for each expression the walker encounters, with populated scope
    fn check_expr(&self, _ctx: &ExprContext) -> Vec<RuleError> { vec![] }
}
