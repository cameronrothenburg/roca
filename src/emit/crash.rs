use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::ast_helpers::{
    ident, string_lit, number_lit, static_field,
    const_decl, let_decl, assign_expr, break_stmt,
    expr_stmt, if_stmt, block, throw_stmt,
    unary_not, console_call, args1, args2,
};
use super::expressions::build_expr;

/// Wrap a call expression with a crash strategy, returning flat statements.
/// Roca functions return [value, err] tuples — crash handlers check the err element.
/// `source_expr` is the original roca expression, needed for retry to rebuild the call.
pub(crate) fn wrap_with_strategy<'a>(
    ast: &AstBuilder<'a>,
    call_expr: Expression<'a>,
    var_name: &str,
    strategy: &roca::CrashHandlerKind,
    source_expr: &roca::Expr,
) -> Vec<Statement<'a>> {
    match strategy {
        roca::CrashHandlerKind::Simple(chain) => {
            wrap_chain(ast, call_expr, var_name, chain, source_expr)
        }
        roca::CrashHandlerKind::Detailed { arms, default } => {
            wrap_detailed(ast, call_expr, var_name, arms, default)
        }
    }
}

fn wrap_chain<'a>(
    ast: &AstBuilder<'a>,
    call_expr: Expression<'a>,
    var_name: &str,
    chain: &roca::CrashChain,
    source_expr: &roca::Expr,
) -> Vec<Statement<'a>> {
    // Find the terminal step (last in chain)
    let terminal = chain.last().unwrap_or(&roca::CrashStep::Halt);
    let has_log = chain.iter().any(|s| matches!(s, roca::CrashStep::Log));

    // Use the terminal step, with log/retry handled inline
    wrap_terminal(ast, call_expr, var_name, terminal, has_log, source_expr)
}

fn wrap_terminal<'a>(
    ast: &AstBuilder<'a>,
    call_expr: Expression<'a>,
    var_name: &str,
    strategy: &roca::CrashStep,
    has_log: bool,
    source_expr: &roca::Expr,
) -> Vec<Statement<'a>> {
    let tmp = format!("_{}_tmp", var_name);
    let err_name = format!("_{}_err", var_name);

    let mut stmts = Vec::new();

    stmts.push(const_decl(ast, &tmp, call_expr));
    let err_access = static_field(ast, &tmp, "err");
    stmts.push(const_decl(ast, &err_name, err_access));
    if has_log {
        let log_test = ident(ast, &err_name);
        let log_call = console_call(ast, "error", args1(ast, ident(ast, &err_name)));
        let log_stmt = expr_stmt(ast, log_call);
        stmts.push(if_stmt(ast, log_test, log_stmt, None));
    }

    match strategy {
        roca::CrashStep::Halt => {
            // if (_err) throw _err;
            let test = ident(ast, &err_name);
            let throw = throw_stmt(ast, ident(ast, &err_name));
            stmts.push(if_stmt(ast, test, throw, None));
            // const var_name = _tmp[0]
            let val_access = static_field(ast, &tmp, "value");
            stmts.push(const_decl(ast, var_name, val_access));
        }
        roca::CrashStep::Skip => {
            // const var_name = _tmp[0] — if err, value will be null
            let val_access = static_field(ast, &tmp, "value");
            stmts.push(const_decl(ast, var_name, val_access));
        }
        roca::CrashStep::Fallback(val) => {
            let val_access = static_field(ast, &tmp, "value");
            let test = ident(ast, &err_name);

            // If fallback is a closure, call it with the error: fn(e)(err)
            let fallback_expr = if matches!(val, roca::Expr::Closure { .. }) {
                let closure = build_expr(ast, val);
                let args = args1(ast, ident(ast, &err_name));
                ast.expression_call(SPAN, closure, NONE, args, false)
            } else {
                build_expr(ast, val)
            };

            let conditional = ast.expression_conditional(SPAN, test, fallback_expr, val_access);
            stmts.push(const_decl(ast, var_name, conditional));
        }
        roca::CrashStep::Panic => {
            // if (_err) { console.error("PANIC:", _err); process.exit(1); }
            let test = ident(ast, &err_name);
            let mut panic_stmts = ast.vec();
            // console.error("PANIC:", _err)
            let console_err = console_call(ast, "error", args2(ast, string_lit(ast, "PANIC:"), ident(ast, &err_name)));
            panic_stmts.push(expr_stmt(ast, console_err));
            // process.exit(1)
            let exit_call = ast.expression_call(
                SPAN,
                static_field(ast, "process", "exit"),
                NONE, args1(ast, number_lit(ast, 1.0)), false,
            );
            panic_stmts.push(expr_stmt(ast, exit_call));
            let consequent = block(ast, panic_stmts);
            stmts.push(if_stmt(ast, test, consequent, None));
            let val_access = static_field(ast, &tmp, "value");
            stmts.push(const_decl(ast, var_name, val_access));
        }
        roca::CrashStep::Log => {
            // log alone as terminal — just log, continue
            let val_access = static_field(ast, &tmp, "value");
            stmts.push(const_decl(ast, var_name, val_access));
        }
        roca::CrashStep::Retry { attempts, .. } => {
            stmts.clear(); // remove the initial _tmp and _err we added above

            stmts.push(let_decl(ast, var_name));
            stmts.push(let_decl(ast, &err_name));

            // for init: let _attempt = 0
            let attempt_pattern = ast.binding_pattern_binding_identifier(SPAN, "_attempt");
            let zero = number_lit(ast, 0.0);
            let init_decl = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, attempt_pattern, NONE, Some(zero), false);
            let init = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(init_decl), false);

            // test: _attempt < N
            let test = ast.expression_binary(
                SPAN,
                ident(ast, "_attempt"),
                BinaryOperator::LessThan,
                number_lit(ast, *attempts as f64),
            );

            // update: _attempt++
            let update_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_attempt")));
            let update = ast.expression_update(SPAN, UpdateOperator::Increment, false, update_target);

            // loop body
            let mut loop_stmts = ast.vec();

            // const _retry_tmp = call() — rebuild from source
            let retry_call = build_expr(ast, source_expr);
            loop_stmts.push(const_decl(ast, "_retry_tmp", retry_call));

            // _err = _retry_tmp[1]
            let err_assign = assign_expr(ast, &err_name, static_field(ast, "_retry_tmp", "err"));
            loop_stmts.push(expr_stmt(ast, err_assign));

            // if (!_err) { var_name = _retry_tmp[0]; break; }
            let not_err = unary_not(ast, ident(ast, &err_name));
            let val_assign = assign_expr(ast, var_name, static_field(ast, "_retry_tmp", "value"));
            let mut success_stmts = ast.vec();
            success_stmts.push(expr_stmt(ast, val_assign));
            success_stmts.push(break_stmt(ast));
            let success_block = block(ast, success_stmts);
            loop_stmts.push(if_stmt(ast, not_err, success_block, None));

            // if (_attempt === N-1) throw _err
            let last_check = ast.expression_binary(
                SPAN,
                ident(ast, "_attempt"),
                BinaryOperator::StrictEquality,
                number_lit(ast, (*attempts - 1) as f64),
            );
            let throw_err = throw_stmt(ast, ident(ast, &err_name));
            loop_stmts.push(if_stmt(ast, last_check, throw_err, None));

            let loop_body = block(ast, loop_stmts);
            let for_init = ForStatementInit::VariableDeclaration(ast.alloc(init));
            stmts.push(ast.statement_for(SPAN, Some(for_init), Some(test), Some(update), loop_body));
        }
    }

    stmts
}

fn wrap_detailed<'a>(
    ast: &AstBuilder<'a>,
    call_expr: Expression<'a>,
    var_name: &str,
    arms: &[roca::CrashArm],
    default: &Option<roca::CrashChain>,
) -> Vec<Statement<'a>> {
    let tmp = format!("_{}_tmp", var_name);
    let err_name = format!("_{}_err", var_name);

    let mut stmts = Vec::new();

    // const _tmp = call()
    stmts.push(const_decl(ast, &tmp, call_expr));

    // const _err = _tmp[1]
    let err_access = static_field(ast, &tmp, "err");
    stmts.push(const_decl(ast, &err_name, err_access));

    // if (_err) { if/else chain based on _err.message }
    let test = ident(ast, &err_name);

    let if_body = build_catch_if_chain(ast, var_name, &tmp, &err_name, arms, default);
    let if_s = if_stmt(ast, test, Statement::BlockStatement(ast.alloc(if_body)), None);
    stmts.push(if_s);

    // const var_name = _tmp[0]  (only reached if no error or after fallback)
    let val_access = static_field(ast, &tmp, "value");
    stmts.push(const_decl(ast, var_name, val_access));

    stmts
}

fn build_catch_if_chain<'a>(
    ast: &AstBuilder<'a>,
    var_name: &str,
    _tmp: &str,
    err_name: &str,
    arms: &[roca::CrashArm],
    default: &Option<roca::CrashChain>,
) -> BlockStatement<'a> {
    let mut stmts = ast.vec();

    let mut result: Option<Statement<'a>> = default.as_ref().and_then(|chain| {
        chain.last().map(|step| strategy_to_stmt(ast, step, var_name, err_name))
    });

    for arm in arms.iter().rev() {
        // _err.name === "err_name"
        let err_name_access = static_field(ast, err_name, "name");
        let expected = string_lit(ast, &arm.err_name);
        let test = ast.expression_binary(SPAN, err_name_access, BinaryOperator::StrictEquality, expected);

        let handler = arm.chain.last()
            .map(|step| strategy_to_stmt(ast, step, var_name, err_name))
            .unwrap_or_else(|| ast.statement_empty(SPAN));
        let mut then_stmts = ast.vec();
        then_stmts.push(handler);
        let consequent = block(ast, then_stmts);

        result = Some(if_stmt(ast, test, consequent, result));
    }

    if let Some(stmt) = result {
        stmts.push(stmt);
    }

    ast.block_statement(SPAN, stmts)
}

fn strategy_to_stmt<'a>(ast: &AstBuilder<'a>, strategy: &roca::CrashStep, _var_name: &str, err_name: &str) -> Statement<'a> {
    match strategy {
        roca::CrashStep::Halt => {
            throw_stmt(ast, ident(ast, err_name))
        }
        roca::CrashStep::Skip | roca::CrashStep::Log => {
            let stmts = ast.vec();
            block(ast, stmts)
        }
        roca::CrashStep::Fallback(val) => {
            let fallback = build_expr(ast, val);
            ast.statement_return(SPAN, Some(fallback))
        }
        roca::CrashStep::Panic => {
            // process.exit(1)
            let exit_call = ast.expression_call(
                SPAN,
                static_field(ast, "process", "exit"),
                NONE, args1(ast, number_lit(ast, 1.0)), false,
            );
            expr_stmt(ast, exit_call)
        }
        roca::CrashStep::Retry { .. } => {
            throw_stmt(ast, ident(ast, err_name))
        }
    }
}
