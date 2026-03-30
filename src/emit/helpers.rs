use oxc_ast::ast::*;
use oxc_ast::AstBuilder;

use super::ast_helpers::{prop, object_expr, string_lit, null_lit};

/// Build { value: left, err: right } result object
pub(crate) fn make_result<'a>(ast: &AstBuilder<'a>, value: Expression<'a>, err: Expression<'a>) -> Expression<'a> {
    let mut props = ast.vec();
    props.push(prop(ast, "value", value));
    props.push(prop(ast, "err", err));
    object_expr(ast, props)
}

/// Build { name: "err_name", message: "err_message" } error object
pub(crate) fn make_error<'a>(ast: &AstBuilder<'a>, name: &str, message: Expression<'a>) -> Expression<'a> {
    let mut props = ast.vec();
    props.push(prop(ast, "name", string_lit(ast, name)));
    props.push(prop(ast, "message", message));
    object_expr(ast, props)
}

/// Build a simple error with name as message (fallback when declaration not found)
pub(crate) fn make_error_simple<'a>(ast: &AstBuilder<'a>, name: &str) -> Expression<'a> {
    make_error(ast, name, string_lit(ast, name))
}

/// Build null literal
pub(crate) fn null<'a>(ast: &AstBuilder<'a>) -> Expression<'a> {
    null_lit(ast)
}
