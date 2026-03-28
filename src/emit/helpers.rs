use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

/// Build [left, right] array expression (value, error tuple)
pub(crate) fn make_tuple<'a>(ast: &AstBuilder<'a>, left: Expression<'a>, right: Expression<'a>) -> Expression<'a> {
    let mut elements = ast.vec();
    elements.push(ArrayExpressionElement::from(left));
    elements.push(ArrayExpressionElement::from(right));
    ast.expression_array(SPAN, elements)
}

/// Build new Error("message")
pub(crate) fn make_error<'a>(ast: &AstBuilder<'a>, message: &str) -> Expression<'a> {
    let msg = ast.str(message);
    let mut args = ast.vec();
    args.push(Argument::from(ast.expression_string_literal(SPAN, msg, None)));
    ast.expression_new(SPAN, ast.expression_identifier(SPAN, "Error"), NONE, args)
}

/// Build null literal
pub(crate) fn null<'a>(ast: &AstBuilder<'a>) -> Expression<'a> {
    ast.expression_null_literal(SPAN)
}
