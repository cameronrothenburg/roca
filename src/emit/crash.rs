use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::expressions::build_expr;

/// Wrap a call expression with a crash strategy, returning flat statements.
/// Roca functions return [value, err] tuples — crash handlers check the err element.
pub(crate) fn wrap_with_strategy<'a>(
    ast: &AstBuilder<'a>,
    call_expr: Expression<'a>,
    var_name: &str,
    strategy: &roca::CrashHandlerKind,
) -> Vec<Statement<'a>> {
    match strategy {
        roca::CrashHandlerKind::Simple(strat) => {
            wrap_simple(ast, call_expr, var_name, strat)
        }
        roca::CrashHandlerKind::Detailed { arms, default } => {
            wrap_detailed(ast, call_expr, var_name, arms, default)
        }
    }
}

fn wrap_simple<'a>(
    ast: &AstBuilder<'a>,
    call_expr: Expression<'a>,
    var_name: &str,
    strategy: &roca::CrashStrategy,
) -> Vec<Statement<'a>> {
    let tmp = format!("_{}_tmp", var_name);
    let err_name = format!("_{}_err", var_name);

    let mut stmts = Vec::new();

    // const _tmp = call()
    stmts.push(make_const_decl(ast, &tmp, call_expr));

    // const _err = _tmp[1]
    let err_access = make_index_access(ast, &tmp, 1);
    stmts.push(make_const_decl(ast, &err_name, err_access));

    match strategy {
        roca::CrashStrategy::Halt => {
            // if (_err) throw _err;
            let test = ast.expression_identifier(SPAN, ast.str(&err_name));
            let throw = ast.statement_throw(SPAN, ast.expression_identifier(SPAN, ast.str(&err_name)));
            stmts.push(ast.statement_if(SPAN, test, throw, None));
            // const var_name = _tmp[0]
            let val_access = make_index_access(ast, &tmp, 0);
            stmts.push(make_const_decl(ast, var_name, val_access));
        }
        roca::CrashStrategy::Skip => {
            // const var_name = _tmp[0] — if err, value will be null
            let val_access = make_index_access(ast, &tmp, 0);
            stmts.push(make_const_decl(ast, var_name, val_access));
        }
        roca::CrashStrategy::Fallback(val) => {
            // const var_name = _err ? fallback : _tmp[0]
            let fallback_expr = build_expr(ast, val);
            let val_access = make_index_access(ast, &tmp, 0);
            let test = ast.expression_identifier(SPAN, ast.str(&err_name));
            let conditional = ast.expression_conditional(SPAN, test, fallback_expr, val_access);
            stmts.push(make_const_decl(ast, var_name, conditional));
        }
        roca::CrashStrategy::Retry { .. } => {
            // For now, same as halt
            let test = ast.expression_identifier(SPAN, ast.str(&err_name));
            let throw = ast.statement_throw(SPAN, ast.expression_identifier(SPAN, ast.str(&err_name)));
            stmts.push(ast.statement_if(SPAN, test, throw, None));
            let val_access = make_index_access(ast, &tmp, 0);
            stmts.push(make_const_decl(ast, var_name, val_access));
        }
    }

    stmts
}

fn wrap_detailed<'a>(
    ast: &AstBuilder<'a>,
    call_expr: Expression<'a>,
    var_name: &str,
    arms: &[roca::CrashArm],
    default: &Option<roca::CrashStrategy>,
) -> Vec<Statement<'a>> {
    let tmp = format!("_{}_tmp", var_name);
    let err_name = format!("_{}_err", var_name);

    let mut stmts = Vec::new();

    // const _tmp = call()
    stmts.push(make_const_decl(ast, &tmp, call_expr));

    // const _err = _tmp[1]
    let err_access = make_index_access(ast, &tmp, 1);
    stmts.push(make_const_decl(ast, &err_name, err_access));

    // if (_err) { if/else chain based on _err.message }
    let test = ast.expression_identifier(SPAN, ast.str(&err_name));

    let if_body = build_catch_if_chain(ast, var_name, &tmp, &err_name, arms, default);
    let if_stmt = ast.statement_if(SPAN, test, Statement::BlockStatement(ast.alloc(if_body)), None);
    stmts.push(if_stmt);

    // const var_name = _tmp[0]  (only reached if no error or after fallback)
    let val_access = make_index_access(ast, &tmp, 0);
    stmts.push(make_const_decl(ast, var_name, val_access));

    stmts
}

fn build_catch_if_chain<'a>(
    ast: &AstBuilder<'a>,
    var_name: &str,
    _tmp: &str,
    err_name: &str,
    arms: &[roca::CrashArm],
    default: &Option<roca::CrashStrategy>,
) -> BlockStatement<'a> {
    let mut stmts = ast.vec();

    // Build if/else chain in reverse
    let mut result: Option<Statement<'a>> = default.as_ref().map(|strat| {
        strategy_to_stmt(ast, strat, var_name, err_name)
    });

    for arm in arms.iter().rev() {
        // _err.message === "err_name"
        let err_msg_str = ast.str(err_name);
        let err_msg = Expression::from(ast.member_expression_static(
            SPAN, ast.expression_identifier(SPAN, err_msg_str), ast.identifier_name(SPAN, "message"), false,
        ));
        let err_name_lit = ast.str(&arm.err_name);
        let expected = ast.expression_string_literal(SPAN, err_name_lit, None);
        let test = ast.expression_binary(SPAN, err_msg, BinaryOperator::StrictEquality, expected);

        let handler = strategy_to_stmt(ast, &arm.strategy, var_name, err_name);
        let mut then_stmts = ast.vec();
        then_stmts.push(handler);
        let consequent = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, then_stmts)));

        result = Some(ast.statement_if(SPAN, test, consequent, result));
    }

    if let Some(stmt) = result {
        stmts.push(stmt);
    }

    ast.block_statement(SPAN, stmts)
}

fn strategy_to_stmt<'a>(ast: &AstBuilder<'a>, strategy: &roca::CrashStrategy, _var_name: &str, err_name: &str) -> Statement<'a> {
    match strategy {
        roca::CrashStrategy::Halt => {
            let n = ast.str(err_name);
            ast.statement_throw(SPAN, ast.expression_identifier(SPAN, n))
        }
        roca::CrashStrategy::Skip => {
            let stmts = ast.vec();
            Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)))
        }
        roca::CrashStrategy::Fallback(val) => {
            // Return the fallback value directly
            let fallback = build_expr(ast, val);
            ast.statement_return(SPAN, Some(fallback))
        }
        roca::CrashStrategy::Retry { .. } => {
            let n = ast.str(err_name);
            ast.statement_throw(SPAN, ast.expression_identifier(SPAN, n))
        }
    }
}

// ─── Helpers ────────────────────────────────────────────

fn make_const_decl<'a>(ast: &AstBuilder<'a>, name: &str, value: Expression<'a>) -> Statement<'a> {
    let n = ast.str(name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, pattern, NONE, Some(value), false);
    let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
    Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))
}

fn make_index_access<'a>(ast: &AstBuilder<'a>, name: &str, index: u32) -> Expression<'a> {
    let n = ast.str(name);
    let obj = ast.expression_identifier(SPAN, n);
    let idx = ast.expression_numeric_literal(SPAN, index as f64, None, NumberBase::Decimal);
    Expression::from(ast.member_expression_computed(SPAN, obj, idx, false))
}
