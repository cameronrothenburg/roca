use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

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

    stmts.push(make_const_decl(ast, &tmp, call_expr));
    let err_access = make_field_access(ast, &tmp, "err");
    stmts.push(make_const_decl(ast, &err_name, err_access));
    if has_log {
        let log_test = ast.expression_identifier(SPAN, ast.str(&err_name));
        let mut log_args = ast.vec();
        log_args.push(Argument::from(ast.expression_identifier(SPAN, ast.str(&err_name))));
        let log_call = ast.expression_call(
            SPAN,
            Expression::from(ast.member_expression_static(
                SPAN, ast.expression_identifier(SPAN, "console"), ast.identifier_name(SPAN, "error"), false,
            )),
            NONE, log_args, false,
        );
        let log_stmt = ast.statement_expression(SPAN, log_call);
        stmts.push(ast.statement_if(SPAN, log_test, log_stmt, None));
    }

    match strategy {
        roca::CrashStep::Halt => {
            // if (_err) throw _err;
            let test = ast.expression_identifier(SPAN, ast.str(&err_name));
            let throw = ast.statement_throw(SPAN, ast.expression_identifier(SPAN, ast.str(&err_name)));
            stmts.push(ast.statement_if(SPAN, test, throw, None));
            // const var_name = _tmp[0]
            let val_access = make_field_access(ast, &tmp, "value");
            stmts.push(make_const_decl(ast, var_name, val_access));
        }
        roca::CrashStep::Skip => {
            // const var_name = _tmp[0] — if err, value will be null
            let val_access = make_field_access(ast, &tmp, "value");
            stmts.push(make_const_decl(ast, var_name, val_access));
        }
        roca::CrashStep::Fallback(val) => {
            let val_access = make_field_access(ast, &tmp, "value");
            let test = ast.expression_identifier(SPAN, ast.str(&err_name));

            // If fallback is a closure, call it with the error: fn(e)(err)
            let fallback_expr = if matches!(val, roca::Expr::Closure { .. }) {
                let closure = build_expr(ast, val);
                let mut args = ast.vec();
                args.push(Argument::from(ast.expression_identifier(SPAN, ast.str(&err_name))));
                ast.expression_call(SPAN, closure, NONE, args, false)
            } else {
                build_expr(ast, val)
            };

            let conditional = ast.expression_conditional(SPAN, test, fallback_expr, val_access);
            stmts.push(make_const_decl(ast, var_name, conditional));
        }
        roca::CrashStep::Panic => {
            // if (_err) { console.error("PANIC:", _err); process.exit(1); }
            let test = ast.expression_identifier(SPAN, ast.str(&err_name));
            let mut panic_stmts = ast.vec();
            // console.error("PANIC:", _err)
            let mut log_args = ast.vec();
            log_args.push(Argument::from(ast.expression_string_literal(SPAN, ast.str("PANIC:"), None)));
            log_args.push(Argument::from(ast.expression_identifier(SPAN, ast.str(&err_name))));
            let console_err = ast.expression_call(
                SPAN,
                Expression::from(ast.member_expression_static(
                    SPAN, ast.expression_identifier(SPAN, "console"), ast.identifier_name(SPAN, "error"), false,
                )),
                NONE, log_args, false,
            );
            panic_stmts.push(ast.statement_expression(SPAN, console_err));
            // process.exit(1)
            let mut exit_args = ast.vec();
            exit_args.push(Argument::from(ast.expression_numeric_literal(SPAN, 1.0, None, NumberBase::Decimal)));
            let exit_call = ast.expression_call(
                SPAN,
                Expression::from(ast.member_expression_static(
                    SPAN, ast.expression_identifier(SPAN, "process"), ast.identifier_name(SPAN, "exit"), false,
                )),
                NONE, exit_args, false,
            );
            panic_stmts.push(ast.statement_expression(SPAN, exit_call));
            let consequent = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, panic_stmts)));
            stmts.push(ast.statement_if(SPAN, test, consequent, None));
            let val_access = make_field_access(ast, &tmp, "value");
            stmts.push(make_const_decl(ast, var_name, val_access));
        }
        roca::CrashStep::Log => {
            // log alone as terminal — just log, continue
            let val_access = make_field_access(ast, &tmp, "value");
            stmts.push(make_const_decl(ast, var_name, val_access));
        }
        roca::CrashStep::Retry { attempts, .. } => {
            // let var_name; let _err;
            // for (let _attempt = 0; _attempt < N; _attempt++) {
            //   const _tmp = call();
            //   _err = _tmp[1];
            //   if (!_err) { var_name = _tmp[0]; break; }
            //   if (_attempt === N-1) throw _err;
            // }
            stmts.clear(); // remove the initial _tmp and _err we added above

            stmts.push(make_let_decl(ast, var_name));
            stmts.push(make_let_decl(ast, &err_name));

            // for init: let _attempt = 0
            let attempt_pattern = ast.binding_pattern_binding_identifier(SPAN, "_attempt");
            let zero = ast.expression_numeric_literal(SPAN, 0.0, None, NumberBase::Decimal);
            let init_decl = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, attempt_pattern, NONE, Some(zero), false);
            let init = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(init_decl), false);

            // test: _attempt < N
            let test = ast.expression_binary(
                SPAN,
                ast.expression_identifier(SPAN, "_attempt"),
                BinaryOperator::LessThan,
                ast.expression_numeric_literal(SPAN, *attempts as f64, None, NumberBase::Decimal),
            );

            // update: _attempt++
            let update_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_attempt")));
            let update = ast.expression_update(SPAN, UpdateOperator::Increment, false, update_target);

            // loop body
            let mut loop_stmts = ast.vec();

            // const _retry_tmp = call() — rebuild from source
            let retry_call = build_expr(ast, source_expr);
            loop_stmts.push(make_const_decl(ast, "_retry_tmp", retry_call));

            // _err = _retry_tmp[1]
            let err_assign = make_assign_expr(ast, &err_name, make_field_access(ast, "_retry_tmp", "err"));
            loop_stmts.push(ast.statement_expression(SPAN, err_assign));

            // if (!_err) { var_name = _retry_tmp[0]; break; }
            let not_err = ast.expression_unary(SPAN, UnaryOperator::LogicalNot, ast.expression_identifier(SPAN, ast.str(&err_name)));
            let val_assign = make_assign_expr(ast, var_name, make_field_access(ast, "_retry_tmp", "value"));
            let mut success_stmts = ast.vec();
            success_stmts.push(ast.statement_expression(SPAN, val_assign));
            success_stmts.push(ast.statement_break(SPAN, None));
            let success_block = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, success_stmts)));
            loop_stmts.push(ast.statement_if(SPAN, not_err, success_block, None));

            // if (_attempt === N-1) throw _err
            let last_check = ast.expression_binary(
                SPAN,
                ast.expression_identifier(SPAN, "_attempt"),
                BinaryOperator::StrictEquality,
                ast.expression_numeric_literal(SPAN, (*attempts - 1) as f64, None, NumberBase::Decimal),
            );
            let throw_err = ast.statement_throw(SPAN, ast.expression_identifier(SPAN, ast.str(&err_name)));
            loop_stmts.push(ast.statement_if(SPAN, last_check, throw_err, None));

            let loop_body = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, loop_stmts)));
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
    stmts.push(make_const_decl(ast, &tmp, call_expr));

    // const _err = _tmp[1]
    let err_access = make_field_access(ast, &tmp, "err");
    stmts.push(make_const_decl(ast, &err_name, err_access));

    // if (_err) { if/else chain based on _err.message }
    let test = ast.expression_identifier(SPAN, ast.str(&err_name));

    let if_body = build_catch_if_chain(ast, var_name, &tmp, &err_name, arms, default);
    let if_stmt = ast.statement_if(SPAN, test, Statement::BlockStatement(ast.alloc(if_body)), None);
    stmts.push(if_stmt);

    // const var_name = _tmp[0]  (only reached if no error or after fallback)
    let val_access = make_field_access(ast, &tmp, "value");
    stmts.push(make_const_decl(ast, var_name, val_access));

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
        let err_ref = ast.str(err_name);
        let err_name_access = Expression::from(ast.member_expression_static(
            SPAN, ast.expression_identifier(SPAN, err_ref), ast.identifier_name(SPAN, "name"), false,
        ));
        let err_name_lit = ast.str(&arm.err_name);
        let expected = ast.expression_string_literal(SPAN, err_name_lit, None);
        let test = ast.expression_binary(SPAN, err_name_access, BinaryOperator::StrictEquality, expected);

        let handler = arm.chain.last()
            .map(|step| strategy_to_stmt(ast, step, var_name, err_name))
            .unwrap_or_else(|| ast.statement_empty(SPAN));
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

fn strategy_to_stmt<'a>(ast: &AstBuilder<'a>, strategy: &roca::CrashStep, _var_name: &str, err_name: &str) -> Statement<'a> {
    match strategy {
        roca::CrashStep::Halt => {
            let n = ast.str(err_name);
            ast.statement_throw(SPAN, ast.expression_identifier(SPAN, n))
        }
        roca::CrashStep::Skip | roca::CrashStep::Log => {
            let stmts = ast.vec();
            Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)))
        }
        roca::CrashStep::Fallback(val) => {
            let fallback = build_expr(ast, val);
            ast.statement_return(SPAN, Some(fallback))
        }
        roca::CrashStep::Panic => {
            // process.exit(1)
            let mut exit_args = ast.vec();
            exit_args.push(Argument::from(ast.expression_numeric_literal(SPAN, 1.0, None, NumberBase::Decimal)));
            let exit_call = ast.expression_call(
                SPAN,
                Expression::from(ast.member_expression_static(
                    SPAN, ast.expression_identifier(SPAN, "process"), ast.identifier_name(SPAN, "exit"), false,
                )),
                NONE, exit_args, false,
            );
            ast.statement_expression(SPAN, exit_call)
        }
        roca::CrashStep::Retry { .. } => {
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

fn make_let_decl<'a>(ast: &AstBuilder<'a>, name: &str) -> Statement<'a> {
    let n = ast.str(name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, pattern, NONE, None, false);
    let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(declarator), false);
    Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))
}

fn make_assign_expr<'a>(ast: &AstBuilder<'a>, name: &str, value: Expression<'a>) -> Expression<'a> {
    let n = ast.str(name);
    let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, n)));
    ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), value)
}

fn make_field_access<'a>(ast: &AstBuilder<'a>, obj_name: &str, field: &'a str) -> Expression<'a> {
    let n = ast.str(obj_name);
    let obj = ast.expression_identifier(SPAN, n);
    Expression::from(ast.member_expression_static(SPAN, obj, ast.identifier_name(SPAN, field), false))
}
