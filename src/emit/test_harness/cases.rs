use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use crate::emit::ast_helpers::{
    ident, string_lit, null_lit,
    field_access, static_field,
    const_decl, expr_stmt, block, if_stmt, try_catch,
    binary, update_inc, console_call,
    arg, args1,
};

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
                    let mut try_stmts = ast.vec();
                    try_stmts.push(expr_stmt(ast, call));
                    try_stmts.push(expr_stmt(ast, update_inc(ast, "_passed")));

                    let fail_msg = format!("FAIL: {}", label);
                    let log_call = console_call(ast, "log", args1(ast, string_lit(ast, &fail_msg)));
                    let mut catch_stmts = ast.vec();
                    catch_stmts.push(expr_stmt(ast, update_inc(ast, "_failed")));
                    catch_stmts.push(expr_stmt(ast, log_call));

                    body.push(try_catch(ast, try_stmts, "_e", catch_stmts));
                }
                count += 1;
            }
            roca::TestCase::IsErr { args, err_name } => {
                let call = build_call(ast, args);
                let err = field_access(ast, call, "err");
                let name_access = field_access(ast, err, "name");
                emit_assert_eq(ast, &label, name_access, string_lit(ast, err_name), body);
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
        oxc_args.push(arg(crate::emit::expressions::build_expr(ast, a)));
    }
    let n = ast.str(name);
    ast.expression_call(SPAN, ast.expression_identifier(SPAN, n), NONE, oxc_args, false)
}

fn build_method_call<'a>(ast: &AstBuilder<'a>, struct_name: &str, method_name: &str, args: &[roca::Expr]) -> Expression<'a> {
    let mut oxc_args = ast.vec();
    for a in args {
        oxc_args.push(arg(crate::emit::expressions::build_expr(ast, a)));
    }
    let callee = static_field(ast, struct_name, ast.str(method_name));
    ast.expression_call(SPAN, callee, NONE, oxc_args, false)
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
    block_stmts.push(const_decl(ast, "_actual", actual));

    let test = binary(ast, ident(ast, "_actual"), BinaryOperator::StrictEquality, expected);

    let mut then_stmts = ast.vec();
    then_stmts.push(expr_stmt(ast, update_inc(ast, "_passed")));
    let consequent = block(ast, then_stmts);

    let fail_msg = format!("FAIL: {}", label);
    let mut fail_args = ast.vec();
    fail_args.push(arg(string_lit(ast, &fail_msg)));
    fail_args.push(arg(string_lit(ast, "got:")));
    fail_args.push(arg(ident(ast, "_actual")));
    let log_call = console_call(ast, "log", fail_args);
    let mut else_stmts = ast.vec();
    else_stmts.push(expr_stmt(ast, update_inc(ast, "_failed")));
    else_stmts.push(expr_stmt(ast, log_call));
    let alternate = block(ast, else_stmts);

    block_stmts.push(if_stmt(ast, test, consequent, Some(alternate)));
    body.push(block(ast, block_stmts));
}

fn emit_assert_null<'a>(
    ast: &AstBuilder<'a>,
    label: &str,
    actual: Expression<'a>,
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) {
    emit_assert_eq(ast, label, actual, null_lit(ast), body);
}
