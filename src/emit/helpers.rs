use oxc_ast::ast::*;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

/// Build { value: left, err: right } result object
pub(crate) fn make_result<'a>(ast: &AstBuilder<'a>, value: Expression<'a>, err: Expression<'a>) -> Expression<'a> {
    let mut props = ast.vec();

    let val_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, "value")));
    props.push(ast.object_property_kind_object_property(
        SPAN, PropertyKind::Init, val_key, value, false, false, false,
    ));

    let err_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, "err")));
    props.push(ast.object_property_kind_object_property(
        SPAN, PropertyKind::Init, err_key, err, false, false, false,
    ));

    ast.expression_object(SPAN, props)
}

/// Build { name: "err_name", message: "err_message" } error object
pub(crate) fn make_error<'a>(ast: &AstBuilder<'a>, name: &str, message: Expression<'a>) -> Expression<'a> {
    let mut props = ast.vec();

    let name_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, "name")));
    let name_val = ast.expression_string_literal(SPAN, ast.str(name), None);
    props.push(ast.object_property_kind_object_property(
        SPAN, PropertyKind::Init, name_key, name_val, false, false, false,
    ));

    let msg_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, "message")));
    props.push(ast.object_property_kind_object_property(
        SPAN, PropertyKind::Init, msg_key, message, false, false, false,
    ));

    ast.expression_object(SPAN, props)
}

/// Build a simple error with name as message (fallback when declaration not found)
pub(crate) fn make_error_simple<'a>(ast: &AstBuilder<'a>, name: &str) -> Expression<'a> {
    let msg = ast.expression_string_literal(SPAN, ast.str(name), None);
    make_error(ast, name, msg)
}

/// Build null literal
pub(crate) fn null<'a>(ast: &AstBuilder<'a>) -> Expression<'a> {
    ast.expression_null_literal(SPAN)
}
