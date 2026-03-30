use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

pub(crate) enum CallKind<'a> {
    Function(&'a str),
    Method(&'a str, &'a str),
}

pub(crate) fn emit_test_cases<'a>(
    ast: &AstBuilder<'a>,
    call_kind: CallKind<'_>,
    returns_err: bool,
    is_async: bool,
    test: &roca::TestBlock,
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) -> usize {
    let label_prefix = match call_kind {
        CallKind::Function(name) => name.to_string(),
        CallKind::Method(s, m) => format!("{}.{}", s, m),
    };

    let mut count = 0;
    for (i, case) in test.cases.iter().enumerate() {
        let label = format!("{}[{}]", label_prefix, i);
        let build_call = |ast: &AstBuilder<'a>, args: &[roca::Expr]| -> Expression<'a> {
            let raw = match call_kind {
                CallKind::Function(name) => build_fn_call(ast, name, args),
                CallKind::Method(s, m) => build_method_call(ast, s, m, args),
            };
            if is_async { ast.expression_await(SPAN, raw) } else { raw }
        };
        match case {
            roca::TestCase::Equals { args, expected } => {
                let call = build_call(ast, args);
                let result = if returns_err { field_access(ast, call, "value") } else { call };
                emit_assert_eq(ast, &label, result, crate::emit::expressions::build_expr(ast, expected), body);
                count += 1;
            }
            roca::TestCase::IsOk { args } => {
                let call = build_call(ast, args);
                if returns_err {
                    emit_assert_null(ast, &label, field_access(ast, call, "err"), body);
                } else {
                    // Non-err function: wrap in try/catch, pass if no throw
                    let pass_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_passed")));
                    let pass_inc = ast.expression_update(SPAN, UpdateOperator::Increment, false, pass_target);
                    let mut try_stmts = ast.vec();
                    try_stmts.push(ast.statement_expression(SPAN, call));
                    try_stmts.push(ast.statement_expression(SPAN, pass_inc));
                    let try_block = ast.block_statement(SPAN, try_stmts);

                    let fail_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_failed")));
                    let fail_inc = ast.expression_update(SPAN, UpdateOperator::Increment, false, fail_target);
                    let fail_msg = ast.str(&format!("FAIL: {}", label));
                    let mut log_args = ast.vec();
                    log_args.push(Argument::from(ast.expression_string_literal(SPAN, fail_msg, None)));
                    let log_call = ast.expression_call(SPAN,
                        Expression::from(ast.member_expression_static(SPAN, ast.expression_identifier(SPAN, "console"), ast.identifier_name(SPAN, "log"), false)),
                        NONE, log_args, false);
                    let mut catch_stmts = ast.vec();
                    catch_stmts.push(ast.statement_expression(SPAN, fail_inc));
                    catch_stmts.push(ast.statement_expression(SPAN, log_call));
                    let catch_body = ast.block_statement(SPAN, catch_stmts);
                    let err_pattern = ast.binding_pattern_binding_identifier(SPAN, "_e");
                    let catch_clause = ast.catch_clause(SPAN, Some(ast.catch_parameter(SPAN, err_pattern, NONE)), catch_body);

                    body.push(ast.statement_try(SPAN, ast.alloc(try_block), Some(ast.alloc(catch_clause)), NONE));
                }
                count += 1;
            }
            roca::TestCase::IsErr { args, err_name } => {
                let call = build_call(ast, args);
                let err = field_access(ast, call, "err");
                let name_access = Expression::from(ast.member_expression_static(
                    SPAN, err, ast.identifier_name(SPAN, "name"), false,
                ));
                emit_assert_eq(ast, &label, name_access, ast.expression_string_literal(SPAN, ast.str(err_name), None), body);
                count += 1;
            }
            _ => {}
        }
    }
    count
}

fn build_fn_call<'a>(ast: &AstBuilder<'a>, name: &str, args: &[roca::Expr]) -> Expression<'a> {
    let mut oxc_args = ast.vec();
    for a in args {
        oxc_args.push(Argument::from(crate::emit::expressions::build_expr(ast, a)));
    }
    let n = ast.str(name);
    ast.expression_call(SPAN, ast.expression_identifier(SPAN, n), NONE, oxc_args, false)
}

fn build_method_call<'a>(ast: &AstBuilder<'a>, struct_name: &str, method_name: &str, args: &[roca::Expr]) -> Expression<'a> {
    let mut oxc_args = ast.vec();
    for a in args {
        oxc_args.push(Argument::from(crate::emit::expressions::build_expr(ast, a)));
    }
    let s = ast.str(struct_name);
    let m = ast.str(method_name);
    let callee = Expression::from(ast.member_expression_static(
        SPAN, ast.expression_identifier(SPAN, s), ast.identifier_name(SPAN, m), false,
    ));
    ast.expression_call(SPAN, callee, NONE, oxc_args, false)
}

fn field_access<'a>(ast: &AstBuilder<'a>, expr: Expression<'a>, field: &'a str) -> Expression<'a> {
    Expression::from(ast.member_expression_static(
        SPAN, expr, ast.identifier_name(SPAN, field), false,
    ))
}

fn emit_assert_eq<'a>(
    ast: &AstBuilder<'a>,
    label: &str,
    actual: Expression<'a>,
    expected: Expression<'a>,
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) {
    // Wrap in block scope so const _actual doesn't collide across assertions
    let mut block_stmts = ast.vec();

    // Store actual in temp so we can log it on failure
    let tmp_pattern = ast.binding_pattern_binding_identifier(SPAN, "_actual");
    let tmp_decl = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, tmp_pattern, NONE, Some(actual), false);
    let tmp_stmt = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(tmp_decl), false);
    block_stmts.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(tmp_stmt))));

    let test = ast.expression_binary(
        SPAN, ast.expression_identifier(SPAN, "_actual"), BinaryOperator::StrictEquality, expected,
    );

    let pass_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_passed")));
    let pass_inc = ast.expression_update(SPAN, UpdateOperator::Increment, false, pass_target);
    let mut then_stmts = ast.vec();
    then_stmts.push(ast.statement_expression(SPAN, pass_inc));
    let consequent = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, then_stmts)));

    let fail_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_failed")));
    let fail_inc = ast.expression_update(SPAN, UpdateOperator::Increment, false, fail_target);
    let fail_msg = ast.str(&format!("FAIL: {}", label));
    let mut fail_args = ast.vec();
    fail_args.push(Argument::from(ast.expression_string_literal(SPAN, fail_msg, None)));
    fail_args.push(Argument::from(ast.expression_string_literal(SPAN, ast.str("got:"), None)));
    fail_args.push(Argument::from(ast.expression_identifier(SPAN, "_actual")));
    let log_call = ast.expression_call(
        SPAN,
        Expression::from(ast.member_expression_static(
            SPAN, ast.expression_identifier(SPAN, "console"), ast.identifier_name(SPAN, "log"), false,
        )),
        NONE, fail_args, false,
    );
    let mut else_stmts = ast.vec();
    else_stmts.push(ast.statement_expression(SPAN, fail_inc));
    else_stmts.push(ast.statement_expression(SPAN, log_call));
    let alternate = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, else_stmts)));

    block_stmts.push(ast.statement_if(SPAN, test, consequent, Some(alternate)));
    body.push(Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, block_stmts))));
}

fn emit_assert_null<'a>(
    ast: &AstBuilder<'a>,
    label: &str,
    actual: Expression<'a>,
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) {
    emit_assert_eq(ast, label, actual, ast.expression_null_literal(SPAN), body);
}
