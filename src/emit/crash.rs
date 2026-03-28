use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::expressions::build_expr;
use super::helpers::null;

/// Wrap a call expression with a crash strategy, returning statements
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
    match strategy {
        roca::CrashStrategy::Halt => {
            // No wrapping — just execute. If it throws, it throws.
            let n = ast.str(var_name);
            let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
            let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, pattern, NONE, Some(call_expr), false);
            let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
            vec![Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))]
        }
        roca::CrashStrategy::Skip => {
            // try { var = call; } catch(_e) { /* skip */ }
            vec![build_try_catch(ast, call_expr, var_name, |ast, _stmts| {
                // empty catch — skip
                let _ = ast;
            })]
        }
        roca::CrashStrategy::Retry { attempts, delay_ms } => {
            vec![build_retry_loop(ast, call_expr, var_name, *attempts, *delay_ms)]
        }
        roca::CrashStrategy::Fallback(val) => {
            let fallback = build_expr(ast, val);
            vec![build_try_catch_with_fallback(ast, call_expr, var_name, fallback)]
        }
    }
}

fn wrap_detailed<'a>(
    ast: &AstBuilder<'a>,
    call_expr: Expression<'a>,
    var_name: &str,
    arms: &[roca::CrashArm],
    default: &Option<roca::CrashStrategy>,
) -> Vec<Statement<'a>> {
    // try { var = call; } catch(_e) { if (_e.message === "err_name") { ... } else ... }
    let n = ast.str(var_name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, pattern, NONE, None, false);
    let var_decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(declarator), false);

    // try block: var_name = call_expr;
    let n2 = ast.str(var_name);
    let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, n2)));
    let assign = ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), call_expr);
    let try_stmt = ast.statement_expression(SPAN, assign);
    let mut try_stmts = ast.vec();
    try_stmts.push(try_stmt);
    let try_block = ast.block_statement(SPAN, try_stmts);

    // catch block: build if/else chain from arms
    let catch_body = build_catch_if_chain(ast, arms, default);
    let err_binding = ast.str("_e");
    let err_pattern = ast.binding_pattern_binding_identifier(SPAN, err_binding);
    let catch_clause = ast.catch_clause(SPAN, Some(ast.catch_parameter(SPAN, err_pattern, NONE)), catch_body);

    let try_catch = ast.statement_try(SPAN, ast.alloc(try_block), Some(ast.alloc(catch_clause)), NONE);

    vec![
        Statement::from(Declaration::VariableDeclaration(ast.alloc(var_decl))),
        try_catch,
    ]
}

fn build_catch_if_chain<'a>(
    ast: &AstBuilder<'a>,
    arms: &[roca::CrashArm],
    default: &Option<roca::CrashStrategy>,
) -> BlockStatement<'a> {
    let mut stmts = ast.vec();

    // Build if/else chain in reverse
    let mut result: Option<Statement<'a>> = default.as_ref().map(|strat| {
        strategy_to_stmt(ast, strat)
    });

    for arm in arms.iter().rev() {
        // _e.message === "err_name"
        let err_msg = Expression::from(ast.member_expression_static(
            SPAN, ast.expression_identifier(SPAN, "_e"), ast.identifier_name(SPAN, "message"), false,
        ));
        let err_name_str = ast.str(&arm.err_name);
        let expected = ast.expression_string_literal(SPAN, err_name_str, None);
        let test = ast.expression_binary(SPAN, err_msg, BinaryOperator::StrictEquality, expected);

        let handler = strategy_to_stmt(ast, &arm.strategy);
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

fn strategy_to_stmt<'a>(ast: &AstBuilder<'a>, strategy: &roca::CrashStrategy) -> Statement<'a> {
    match strategy {
        roca::CrashStrategy::Halt => {
            // throw _e
            let err = ast.expression_identifier(SPAN, "_e");
            ast.statement_throw(SPAN, err)
        }
        roca::CrashStrategy::Skip => {
            // empty — just return
            let mut stmts = ast.vec();
            Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)))
        }
        roca::CrashStrategy::Fallback(val) => {
            let fallback = build_expr(ast, val);
            ast.statement_return(SPAN, Some(fallback))
        }
        roca::CrashStrategy::Retry { .. } => {
            // For nested retry inside detailed handler, just rethrow for now
            let err = ast.expression_identifier(SPAN, "_e");
            ast.statement_throw(SPAN, err)
        }
    }
}

fn build_try_catch<'a, F>(
    ast: &AstBuilder<'a>,
    call_expr: Expression<'a>,
    var_name: &str,
    _catch_handler: F,
) -> Statement<'a>
where
    F: FnOnce(&AstBuilder<'a>, &mut oxc_allocator::Vec<'a, Statement<'a>>),
{
    let n = ast.str(var_name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, pattern, NONE, None, false);
    let var_decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(declarator), false);

    let n2 = ast.str(var_name);
    let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, n2)));
    let assign = ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), call_expr);
    let mut try_stmts = ast.vec();
    try_stmts.push(ast.statement_expression(SPAN, assign));
    let try_block = ast.block_statement(SPAN, try_stmts);

    let catch_stmts = ast.vec();
    let catch_body = ast.block_statement(SPAN, catch_stmts);
    let err_binding = ast.str("_e");
    let err_pattern = ast.binding_pattern_binding_identifier(SPAN, err_binding);
    let catch_clause = ast.catch_clause(SPAN, Some(ast.catch_parameter(SPAN, err_pattern, NONE)), catch_body);

    ast.statement_try(SPAN, ast.alloc(try_block), Some(ast.alloc(catch_clause)), NONE)
}

fn build_try_catch_with_fallback<'a>(
    ast: &AstBuilder<'a>,
    call_expr: Expression<'a>,
    var_name: &str,
    fallback: Expression<'a>,
) -> Statement<'a> {
    let n = ast.str(var_name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, pattern, NONE, None, false);
    let var_decl_stmt = Statement::from(Declaration::VariableDeclaration(ast.alloc(
        ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(declarator), false),
    )));

    let n2 = ast.str(var_name);
    let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, n2)));
    let assign = ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), call_expr);
    let mut try_stmts = ast.vec();
    try_stmts.push(ast.statement_expression(SPAN, assign));
    let try_block = ast.block_statement(SPAN, try_stmts);

    // catch: var_name = fallback
    let n3 = ast.str(var_name);
    let target2 = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, n3)));
    let assign2 = ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target2), fallback);
    let mut catch_stmts = ast.vec();
    catch_stmts.push(ast.statement_expression(SPAN, assign2));
    let catch_body = ast.block_statement(SPAN, catch_stmts);
    let err_binding = ast.str("_e");
    let err_pattern = ast.binding_pattern_binding_identifier(SPAN, err_binding);
    let catch_clause = ast.catch_clause(SPAN, Some(ast.catch_parameter(SPAN, err_pattern, NONE)), catch_body);

    // We need both the var decl and the try/catch. Return just try/catch — caller handles var decl.
    // Actually, we can't return two statements. Let's use a block.
    let mut block_stmts = ast.vec();
    block_stmts.push(var_decl_stmt);
    block_stmts.push(ast.statement_try(SPAN, ast.alloc(try_block), Some(ast.alloc(catch_clause)), NONE));
    Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, block_stmts)))
}

fn build_retry_loop<'a>(
    ast: &AstBuilder<'a>,
    call_expr: Expression<'a>,
    var_name: &str,
    attempts: u32,
    delay_ms: u32,
) -> Statement<'a> {
    // for (let _attempt = 0; _attempt < N; _attempt++) {
    //   try { var = call; break; } catch(_e) { if (_attempt === N-1) throw _e; await sleep(ms); }
    // }
    // For now, emit a simpler version without async:
    // let var; for (let _attempt = 0; _attempt < N; _attempt++) { try { var = call; break; } catch(_e) { if (_attempt === N-1) throw _e; } }

    let n = ast.str(var_name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, pattern, NONE, None, false);
    let var_decl = Statement::from(Declaration::VariableDeclaration(ast.alloc(
        ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(declarator), false),
    )));

    // Init: let _attempt = 0
    let attempt_pattern = ast.binding_pattern_binding_identifier(SPAN, "_attempt");
    let zero = ast.expression_numeric_literal(SPAN, 0.0, None, NumberBase::Decimal);
    let init_declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, attempt_pattern, NONE, Some(zero), false);
    let init = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(init_declarator), false);

    // Test: _attempt < N
    let test = ast.expression_binary(
        SPAN,
        ast.expression_identifier(SPAN, "_attempt"),
        BinaryOperator::LessThan,
        ast.expression_numeric_literal(SPAN, attempts as f64, None, NumberBase::Decimal),
    );

    // Update: _attempt++
    let update_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, "_attempt")));
    let update = ast.expression_update(SPAN, UpdateOperator::Increment, false, update_target);

    // Try block: var_name = call; break;
    let n2 = ast.str(var_name);
    let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, n2)));
    let assign = ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), call_expr);
    let mut try_stmts = ast.vec();
    try_stmts.push(ast.statement_expression(SPAN, assign));
    try_stmts.push(ast.statement_break(SPAN, None));
    let try_block = ast.block_statement(SPAN, try_stmts);

    // Catch: if (_attempt === N-1) throw _e;
    let last_attempt = ast.expression_binary(
        SPAN,
        ast.expression_identifier(SPAN, "_attempt"),
        BinaryOperator::StrictEquality,
        ast.expression_numeric_literal(SPAN, (attempts - 1) as f64, None, NumberBase::Decimal),
    );
    let throw_e = ast.statement_throw(SPAN, ast.expression_identifier(SPAN, "_e"));
    let if_last = ast.statement_if(SPAN, last_attempt, throw_e, None);
    let mut catch_stmts = ast.vec();
    catch_stmts.push(if_last);
    let catch_body = ast.block_statement(SPAN, catch_stmts);
    let err_binding = ast.str("_e");
    let err_pattern = ast.binding_pattern_binding_identifier(SPAN, err_binding);
    let catch_clause = ast.catch_clause(SPAN, Some(ast.catch_parameter(SPAN, err_pattern, NONE)), catch_body);

    let try_catch = ast.statement_try(SPAN, ast.alloc(try_block), Some(ast.alloc(catch_clause)), NONE);
    let mut for_stmts = ast.vec();
    for_stmts.push(try_catch);
    let for_body = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, for_stmts)));

    let for_init = ForStatementInit::VariableDeclaration(ast.alloc(init));
    let for_stmt = ast.statement_for(SPAN, Some(for_init), Some(test), Some(update), for_body);

    let mut block = ast.vec();
    block.push(var_decl);
    block.push(for_stmt);
    Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, block)))
}
