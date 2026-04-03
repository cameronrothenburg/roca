//! Expression codegen — thin wrapper over shapes::expr_to_js.
//! All shape-specific logic lives in shapes.rs for single-source-of-truth.

use roca_ast as roca;
use oxc_ast::ast::*;
use oxc_ast::AstBuilder;

/// Build a JS expression from a Roca expression.
/// Delegates to `shapes::expr_to_js` — each shape has its own function.
pub(crate) fn build_expr<'a>(ast: &AstBuilder<'a>, expr: &roca::Expr) -> Expression<'a> {
    super::shapes::expr_to_js(ast, expr)
}
