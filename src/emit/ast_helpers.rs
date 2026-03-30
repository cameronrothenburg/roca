use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

// ─── Expressions ───────────────────────────────────────────

pub(crate) fn ident<'a>(ast: &AstBuilder<'a>, name: &str) -> Expression<'a> {
    let n = ast.str(name);
    ast.expression_identifier(SPAN, n)
}

pub(crate) fn string_lit<'a>(ast: &AstBuilder<'a>, s: &str) -> Expression<'a> {
    let s = ast.str(s);
    ast.expression_string_literal(SPAN, s, None)
}

pub(crate) fn number_lit<'a>(ast: &AstBuilder<'a>, n: f64) -> Expression<'a> {
    ast.expression_numeric_literal(SPAN, n, None, NumberBase::Decimal)
}

pub(crate) fn bool_lit<'a>(ast: &AstBuilder<'a>, b: bool) -> Expression<'a> {
    ast.expression_boolean_literal(SPAN, b)
}

pub(crate) fn null_lit<'a>(ast: &AstBuilder<'a>) -> Expression<'a> {
    ast.expression_null_literal(SPAN)
}

pub(crate) fn field_access<'a>(ast: &AstBuilder<'a>, obj: Expression<'a>, field: &'a str) -> Expression<'a> {
    Expression::from(ast.member_expression_static(SPAN, obj, ast.identifier_name(SPAN, field), false))
}

pub(crate) fn static_field<'a>(ast: &AstBuilder<'a>, obj_name: &str, field: &'a str) -> Expression<'a> {
    let obj = ident(ast, obj_name);
    field_access(ast, obj, field)
}

pub(crate) fn binary<'a>(ast: &AstBuilder<'a>, left: Expression<'a>, op: BinaryOperator, right: Expression<'a>) -> Expression<'a> {
    ast.expression_binary(SPAN, left, op, right)
}

pub(crate) fn unary_not<'a>(ast: &AstBuilder<'a>, expr: Expression<'a>) -> Expression<'a> {
    ast.expression_unary(SPAN, UnaryOperator::LogicalNot, expr)
}

pub(crate) fn object_expr<'a>(ast: &AstBuilder<'a>, props: oxc_allocator::Vec<'a, ObjectPropertyKind<'a>>) -> Expression<'a> {
    ast.expression_object(SPAN, props)
}

pub(crate) fn prop<'a>(ast: &AstBuilder<'a>, key: &str, value: Expression<'a>) -> ObjectPropertyKind<'a> {
    let k = ast.str(key);
    let prop_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, k)));
    ast.object_property_kind_object_property(SPAN, PropertyKind::Init, prop_key, value, false, false, false)
}

pub(crate) fn assign_expr<'a>(ast: &AstBuilder<'a>, name: &str, value: Expression<'a>) -> Expression<'a> {
    let n = ast.str(name);
    let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, n)));
    ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), value)
}

pub(crate) fn update_inc<'a>(ast: &AstBuilder<'a>, name: &'a str) -> Expression<'a> {
    let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, name)));
    ast.expression_update(SPAN, UpdateOperator::Increment, false, target)
}

pub(crate) fn console_call<'a>(ast: &AstBuilder<'a>, method: &'a str, args: oxc_allocator::Vec<'a, Argument<'a>>) -> Expression<'a> {
    let callee = static_field(ast, "console", method);
    ast.expression_call(SPAN, callee, NONE, args, false)
}

// ─── Statements ────────────────────────────────────────────

pub(crate) fn const_decl<'a>(ast: &AstBuilder<'a>, name: &str, value: Expression<'a>) -> Statement<'a> {
    let n = ast.str(name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, pattern, NONE, Some(value), false);
    let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
    Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))
}

pub(crate) fn let_decl<'a>(ast: &AstBuilder<'a>, name: &str) -> Statement<'a> {
    let n = ast.str(name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, pattern, NONE, None, false);
    let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(declarator), false);
    Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))
}

pub(crate) fn expr_stmt<'a>(ast: &AstBuilder<'a>, expr: Expression<'a>) -> Statement<'a> {
    ast.statement_expression(SPAN, expr)
}

pub(crate) fn return_stmt<'a>(ast: &AstBuilder<'a>, value: Expression<'a>) -> Statement<'a> {
    ast.statement_return(SPAN, Some(value))
}

pub(crate) fn throw_stmt<'a>(ast: &AstBuilder<'a>, value: Expression<'a>) -> Statement<'a> {
    ast.statement_throw(SPAN, value)
}

pub(crate) fn if_stmt<'a>(ast: &AstBuilder<'a>, test: Expression<'a>, consequent: Statement<'a>, alternate: Option<Statement<'a>>) -> Statement<'a> {
    ast.statement_if(SPAN, test, consequent, alternate)
}

pub(crate) fn block<'a>(ast: &AstBuilder<'a>, stmts: oxc_allocator::Vec<'a, Statement<'a>>) -> Statement<'a> {
    Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)))
}

pub(crate) fn try_catch<'a>(ast: &AstBuilder<'a>, try_stmts: oxc_allocator::Vec<'a, Statement<'a>>, catch_var: &'a str, catch_stmts: oxc_allocator::Vec<'a, Statement<'a>>) -> Statement<'a> {
    let try_block = ast.block_statement(SPAN, try_stmts);
    let catch_body = ast.block_statement(SPAN, catch_stmts);
    let err_pattern = ast.binding_pattern_binding_identifier(SPAN, catch_var);
    let catch_clause = ast.catch_clause(SPAN, Some(ast.catch_parameter(SPAN, err_pattern, NONE)), catch_body);
    ast.statement_try(SPAN, ast.alloc(try_block), Some(ast.alloc(catch_clause)), NONE)
}

pub(crate) fn break_stmt<'a>(ast: &AstBuilder<'a>) -> Statement<'a> {
    ast.statement_break(SPAN, None)
}

// ─── Declarations ──────────────────────────────────────────

pub(crate) fn param<'a>(ast: &AstBuilder<'a>, name: &str) -> FormalParameter<'a> {
    let pn = ast.str(name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, pn);
    ast.plain_formal_parameter(SPAN, pattern)
}

pub(crate) fn formal_params<'a>(ast: &AstBuilder<'a>, params: oxc_allocator::Vec<'a, FormalParameter<'a>>) -> FormalParameters<'a> {
    ast.formal_parameters(SPAN, FormalParameterKind::FormalParameter, params, NONE)
}

pub(crate) fn function_body<'a>(ast: &AstBuilder<'a>, stmts: oxc_allocator::Vec<'a, Statement<'a>>) -> FunctionBody<'a> {
    ast.function_body(SPAN, ast.vec(), stmts)
}

pub(crate) fn function_expr<'a>(ast: &AstBuilder<'a>, params: FormalParameters<'a>, body: FunctionBody<'a>, is_async: bool) -> Expression<'a> {
    let func = ast.function(
        SPAN, FunctionType::FunctionExpression, None, false, is_async, false,
        NONE, NONE, params, NONE, Some(body),
    );
    Expression::FunctionExpression(ast.alloc(func))
}

pub(crate) fn prop_key<'a>(ast: &AstBuilder<'a>, name: &str) -> PropertyKey<'a> {
    let k = ast.str(name);
    PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, k)))
}

// ─── Argument helpers ──────────────────────────────────────

pub(crate) fn arg<'a>(expr: Expression<'a>) -> Argument<'a> {
    Argument::from(expr)
}

pub(crate) fn args1<'a>(ast: &AstBuilder<'a>, a: Expression<'a>) -> oxc_allocator::Vec<'a, Argument<'a>> {
    let mut args = ast.vec();
    args.push(Argument::from(a));
    args
}

pub(crate) fn args2<'a>(ast: &AstBuilder<'a>, a: Expression<'a>, b: Expression<'a>) -> oxc_allocator::Vec<'a, Argument<'a>> {
    let mut args = ast.vec();
    args.push(Argument::from(a));
    args.push(Argument::from(b));
    args
}
