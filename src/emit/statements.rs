use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::expressions::build_expr;
use super::crash::wrap_with_strategy;
use super::helpers::{make_result, make_error, null};

fn zero_value<'a>(ast: &AstBuilder<'a>, t: &roca::TypeRef) -> Expression<'a> {
    match t {
        roca::TypeRef::String => ast.expression_string_literal(SPAN, ast.str(""), None),
        roca::TypeRef::Number => ast.expression_numeric_literal(SPAN, 0.0, None, NumberBase::Decimal),
        roca::TypeRef::Bool => ast.expression_boolean_literal(SPAN, false),
        roca::TypeRef::Nullable(_) => ast.expression_null_literal(SPAN),
        _ => ast.expression_null_literal(SPAN),
    }
}

fn lookup_error_message<'a>(name: &str, errors: &[roca::ErrDecl]) -> Option<String> {
    errors.iter().find(|e| e.name == name).map(|e| e.message.clone())
}

use std::sync::atomic::{AtomicUsize, Ordering};
static CRASH_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn build_stmt<'a>(
    ast: &AstBuilder<'a>,
    stmt: &roca::Stmt,
    returns_err: bool,
    return_type: &roca::TypeRef,
    errors_decl: &[roca::ErrDecl],
    crash: Option<&roca::CrashBlock>,
) -> Vec<Statement<'a>> {
    match stmt {
        roca::Stmt::Const { name, value, .. } => {
            emit_var_decl(ast, name, value, VariableDeclarationKind::Const, crash)
        }
        roca::Stmt::Let { name, value, .. } => {
            emit_var_decl(ast, name, value, VariableDeclarationKind::Let, crash)
        }
        roca::Stmt::LetResult { name, err_name, value } => {
            if let Some((cast_type, input)) = extract_cast_input(value) {
                return emit_safe_cast(ast, name, err_name, &cast_type, input);
            }

            let n = ast.str(name);
            let e = ast.str(err_name);
            let mut props = ast.vec();
            // { value: name, err: err_name }
            let val_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, "value")));
            let val_bind = ast.binding_pattern_binding_identifier(SPAN, n);
            props.push(ast.binding_property(SPAN, val_key, val_bind, false, false));
            let err_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, "err")));
            let err_bind = ast.binding_pattern_binding_identifier(SPAN, e);
            props.push(ast.binding_property(SPAN, err_key, err_bind, false, false));
            let obj_pattern = ast.binding_pattern_object_pattern(SPAN, props, NONE);
            let init = build_expr(ast, value);
            let handler = find_crash_handler(value, crash);
            let terminal = handler.and_then(|h| match &h.strategy {
                roca::CrashHandlerKind::Simple(chain) => chain.last(),
                _ => None,
            });
            let has_fallback = matches!(terminal, Some(roca::CrashStep::Fallback(_)));
            let decl_kind = if has_fallback { VariableDeclarationKind::Let } else { VariableDeclarationKind::Const };
            let declarator = ast.variable_declarator(SPAN, decl_kind, obj_pattern, NONE, Some(init), false);
            let decl = ast.variable_declaration(SPAN, decl_kind, ast.vec1(declarator), false);
            let mut result = vec![Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))];

            if let Some(terminal) = terminal {
                    match terminal {
                        roca::CrashStep::Halt if returns_err => {
                            let err_check = ast.expression_identifier(SPAN, ast.str(err_name));
                            let zero = zero_value(ast, return_type);
                            let propagate_err = ast.expression_identifier(SPAN, ast.str(err_name));
                            let ret = make_result(ast, zero, propagate_err);
                            let mut then = ast.vec();
                            then.push(ast.statement_return(SPAN, Some(ret)));
                            let consequent = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, then)));
                            result.push(ast.statement_if(SPAN, err_check, consequent, None));
                        }
                        roca::CrashStep::Fallback(fallback_expr) => {
                            let err_check = ast.expression_identifier(SPAN, ast.str(err_name));
                            let fb_val = if matches!(fallback_expr, roca::Expr::Closure { .. }) {
                                let closure = build_expr(ast, fallback_expr);
                                let mut args = ast.vec();
                                args.push(Argument::from(ast.expression_identifier(SPAN, ast.str(err_name))));
                                ast.expression_call(SPAN, closure, NONE, args, false)
                            } else {
                                build_expr(ast, fallback_expr)
                            };
                            let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(
                                ast.alloc(ast.identifier_reference(SPAN, ast.str(name)))
                            );
                            let assign = ast.expression_assignment(
                                SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), fb_val,
                            );
                            let mut then = ast.vec();
                            then.push(ast.statement_expression(SPAN, assign));
                            let consequent = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, then)));
                            result.push(ast.statement_if(SPAN, err_check, consequent, None));
                        }
                        roca::CrashStep::Panic => {
                            let err_check = ast.expression_identifier(SPAN, ast.str(err_name));
                            let mut panic_stmts = ast.vec();
                            let mut log_args = ast.vec();
                            log_args.push(Argument::from(ast.expression_string_literal(SPAN, ast.str("PANIC:"), None)));
                            log_args.push(Argument::from(ast.expression_identifier(SPAN, ast.str(err_name))));
                            let console_err = ast.expression_call(SPAN,
                                Expression::from(ast.member_expression_static(SPAN, ast.expression_identifier(SPAN, "console"), ast.identifier_name(SPAN, "error"), false)),
                                NONE, log_args, false);
                            panic_stmts.push(ast.statement_expression(SPAN, console_err));
                            let mut exit_args = ast.vec();
                            exit_args.push(Argument::from(ast.expression_numeric_literal(SPAN, 1.0, None, NumberBase::Decimal)));
                            let exit_call = ast.expression_call(SPAN,
                                Expression::from(ast.member_expression_static(SPAN, ast.expression_identifier(SPAN, "process"), ast.identifier_name(SPAN, "exit"), false)),
                                NONE, exit_args, false);
                            panic_stmts.push(ast.statement_expression(SPAN, exit_call));
                            let consequent = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, panic_stmts)));
                            result.push(ast.statement_if(SPAN, err_check, consequent, None));
                        }
                        roca::CrashStep::Skip => {}
                        _ => {}
                    }
            }

            result
        }
        roca::Stmt::Return(expr) => {
            // Check if the return expression is a crash-handled call with fallback
            // Fallback changes the value, so we need to unwrap before returning
            if let Some(handler) = find_crash_handler(expr, crash) {
                let is_fallback = match &handler.strategy {
                    roca::CrashHandlerKind::Simple(chain) => chain.iter().any(|s| matches!(s, roca::CrashStep::Fallback(_))),
                    _ => false,
                };
                if is_fallback {
                    let call_expr = build_expr(ast, expr);
                    let tmp_name = "_ret";
                    let mut stmts = wrap_with_strategy(ast, call_expr, tmp_name, &handler.strategy, expr);
                    let ret_val = ast.expression_identifier(SPAN, tmp_name);
                    if returns_err {
                        stmts.push(ast.statement_return(SPAN, Some(make_result(ast, ret_val, null(ast)))));
                    } else {
                        stmts.push(ast.statement_return(SPAN, Some(ret_val)));
                    }
                    return stmts;
                }
            }

            let val = build_expr(ast, expr);
            // If expression is a match with err arms, it already produces tuples — don't double-wrap
            let already_tupled = if let roca::Expr::Match { arms, .. } = expr {
                super::expressions::match_has_err_arms(arms)
            } else {
                false
            };
            if returns_err && !already_tupled {
                let ret = make_result(ast, val, null(ast));
                vec![ast.statement_return(SPAN, Some(ret))]
            } else {
                vec![ast.statement_return(SPAN, Some(val))]
            }
        }
        roca::Stmt::ReturnErr { name: err_name, custom_message } => {
            let msg_expr = if let Some(custom) = custom_message {
                build_expr(ast, custom)
            } else {
                let default_msg = lookup_error_message(err_name, errors_decl)
                    .unwrap_or_else(|| err_name.clone());
                ast.expression_string_literal(SPAN, ast.str(&default_msg), None)
            };
            let err = make_error(ast, err_name, msg_expr);
            let zero = zero_value(ast, return_type);
            let ret = make_result(ast, zero, err);
            vec![ast.statement_return(SPAN, Some(ret))]
        }
        roca::Stmt::Assign { name, value } => {
            let n = ast.str(name);
            let id_ref = ast.identifier_reference(SPAN, n);
            let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(id_ref));
            let val = build_expr(ast, value);
            let assign = ast.expression_assignment(
                SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), val,
            );
            vec![ast.statement_expression(SPAN, assign)]
        }
        roca::Stmt::FieldAssign { target: obj, field, value } => {
            let obj_expr = build_expr(ast, obj);
            let f = ast.str(field);
            let member = ast.member_expression_static(SPAN, obj_expr, ast.identifier_name(SPAN, f), false);
            let target = SimpleAssignmentTarget::from(member);
            let val = build_expr(ast, value);
            let assign = ast.expression_assignment(
                SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), val,
            );
            vec![ast.statement_expression(SPAN, assign)]
        }
        roca::Stmt::Expr(expr) => {
            if let Some(handler) = find_crash_handler(expr, crash) {
                if !is_passthrough(&handler.strategy) {
                    let call_expr = build_expr(ast, expr);
                    let id = CRASH_COUNTER.fetch_add(1, Ordering::Relaxed);
                    let var_name = format!("_r{}", id);
                    return wrap_with_strategy(ast, call_expr, &var_name, &handler.strategy, expr);
                }
            }
            let val = build_expr(ast, expr);
            vec![ast.statement_expression(SPAN, val)]
        }
        roca::Stmt::If { condition, then_body, else_body } => {
            let test = build_expr(ast, condition);
            let mut then_stmts = ast.vec();
            for s in then_body {
                for emitted in build_stmt(ast, s, returns_err, return_type, errors_decl, crash) {
                    then_stmts.push(emitted);
                }
            }
            let consequent = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, then_stmts)));

            let alternate = else_body.as_ref().map(|body| {
                let mut stmts = ast.vec();
                for s in body {
                    for emitted in build_stmt(ast, s, returns_err, return_type, errors_decl, crash) {
                        stmts.push(emitted);
                    }
                }
                Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)))
            });

            vec![ast.statement_if(SPAN, test, consequent, alternate)]
        }
        roca::Stmt::For { binding, iter, body } => {
            let b = ast.str(binding);
            let pattern = ast.binding_pattern_binding_identifier(SPAN, b);
            let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, pattern, NONE, None, false);
            let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
            let left = ForStatementLeft::VariableDeclaration(ast.alloc(decl));
            let right = build_expr(ast, iter);
            let mut stmts = ast.vec();
            for s in body {
                for emitted in build_stmt(ast, s, returns_err, return_type, errors_decl, crash) {
                    stmts.push(emitted);
                }
            }
            let body_stmt = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)));
            vec![ast.statement_for_of(SPAN, false, left, right, body_stmt)]
        }
        roca::Stmt::While { condition, body } => {
            let test = build_expr(ast, condition);
            let mut stmts = ast.vec();
            for s in body {
                for emitted in build_stmt(ast, s, returns_err, return_type, errors_decl, crash) {
                    stmts.push(emitted);
                }
            }
            let body_stmt = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)));
            vec![ast.statement_while(SPAN, test, body_stmt)]
        }
        roca::Stmt::Break => {
            vec![ast.statement_break(SPAN, None)]
        }
        roca::Stmt::Continue => {
            vec![ast.statement_continue(SPAN, None)]
        }
        roca::Stmt::Wait { names, failed_name, kind } => {
            emit_wait(ast, names, failed_name, kind)
        }
    }
}

/// Check if an expression is a primitive type cast: String(x), Number(x), Bool(x)
fn emit_var_decl<'a>(
    ast: &AstBuilder<'a>,
    name: &str,
    value: &roca::Expr,
    kind: VariableDeclarationKind,
    crash: Option<&roca::CrashBlock>,
) -> Vec<Statement<'a>> {
    if let Some(handler) = find_crash_handler(value, crash) {
        if !is_passthrough(&handler.strategy) {
            let call_expr = build_expr(ast, value);
            return wrap_with_strategy(ast, call_expr, name, &handler.strategy, value);
        }
    }
    let n = ast.str(name);
    let init = build_expr(ast, value);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, kind, pattern, NONE, Some(init), false);
    let decl = ast.variable_declaration(SPAN, kind, ast.vec1(declarator), false);
    vec![Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))]
}

fn extract_cast_input<'a>(expr: &'a roca::Expr) -> Option<(String, &'a roca::Expr)> {
    if let roca::Expr::Call { target, args } = expr {
        if let roca::Expr::Ident(name) = target.as_ref() {
            if matches!(name.as_str(), "String" | "Number" | "Bool") && args.len() == 1 {
                return Some((name.clone(), &args[0]));
            }
        }
    }
    None
}

/// Emit a safe type cast with null/NaN checking:
/// let value; let err;
/// try {
///   const _raw = Type(input);
///   if (input === null || input === undefined || <type-specific check>) { err = new Error("invalid_cast"); }
///   else { value = _raw; }
/// } catch(_e) { err = _e; }
fn emit_safe_cast<'a>(
    ast: &AstBuilder<'a>,
    name: &str,
    err_name: &str,
    cast_type: &str,
    input_expr: &roca::Expr,
) -> Vec<Statement<'a>> {
    let mut stmts = Vec::new();

    // let value; let err;
    let n = ast.str(name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let decl = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, pattern, NONE, None, false);
    stmts.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(
        ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(decl), false),
    ))));

    let en = ast.str(err_name);
    let e_pattern = ast.binding_pattern_binding_identifier(SPAN, en);
    let e_decl = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, e_pattern, NONE, None, false);
    stmts.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(
        ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(e_decl), false),
    ))));

    let mut try_stmts = ast.vec();

    // const _input = <input expr>; — build once, reference twice
    let input_val = build_expr(ast, input_expr);
    let inp_pattern = ast.binding_pattern_binding_identifier(SPAN, "_input");
    let inp_decl = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, inp_pattern, NONE, Some(input_val), false);
    try_stmts.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(
        ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(inp_decl), false),
    ))));

    // const _raw = Type(_input)
    let js_type = if cast_type == "Bool" { "Boolean" } else { cast_type };
    let type_name = ast.str(js_type);
    let mut cast_args = ast.vec();
    cast_args.push(Argument::from(ast.expression_identifier(SPAN, "_input")));
    let cast_call = ast.expression_call(SPAN, ast.expression_identifier(SPAN, type_name), NONE, cast_args, false);
    let raw_pattern = ast.binding_pattern_binding_identifier(SPAN, "_raw");
    let raw_decl = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, raw_pattern, NONE, Some(cast_call), false);
    try_stmts.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(
        ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(raw_decl), false),
    ))));

    // null check: _input === null || _input === undefined
    let null_check = ast.expression_binary(SPAN, ast.expression_identifier(SPAN, "_input"), BinaryOperator::StrictEquality, ast.expression_null_literal(SPAN));
    let undef_check = ast.expression_binary(SPAN, ast.expression_identifier(SPAN, "_input"), BinaryOperator::StrictEquality, ast.expression_identifier(SPAN, "undefined"));
    let mut condition = ast.expression_logical(SPAN, null_check, LogicalOperator::Or, undef_check);

    // Type-specific check
    if cast_type == "Number" {
        // Also check isNaN
        let mut nan_args = ast.vec();
        nan_args.push(Argument::from(ast.expression_identifier(SPAN, "_raw")));
        let nan_check = ast.expression_call(
            SPAN,
            Expression::from(ast.member_expression_static(
                SPAN, ast.expression_identifier(SPAN, "Number"), ast.identifier_name(SPAN, "isNaN"), false,
            )),
            NONE, nan_args, false,
        );
        condition = ast.expression_logical(SPAN, condition, LogicalOperator::Or, nan_check);
    }

    // if (condition) { err = new Error("invalid_cast"); } else { value = _raw; }
    let err_assign_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, ast.str(err_name))));
    let error_msg = ast.str(&format!("invalid_{}", cast_type.to_lowercase()));
    let mut err_args = ast.vec();
    err_args.push(Argument::from(ast.expression_string_literal(SPAN, error_msg, None)));
    let new_error = ast.expression_new(SPAN, ast.expression_identifier(SPAN, "Error"), NONE, err_args);
    let err_assign = ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(err_assign_target), new_error);
    let mut then_stmts = ast.vec();
    then_stmts.push(ast.statement_expression(SPAN, err_assign));
    let then_block = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, then_stmts)));

    let val_assign_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, ast.str(name))));
    let val_assign = ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(val_assign_target), ast.expression_identifier(SPAN, "_raw"));
    let mut else_stmts = ast.vec();
    else_stmts.push(ast.statement_expression(SPAN, val_assign));
    let else_block = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, else_stmts)));

    try_stmts.push(ast.statement_if(SPAN, condition, then_block, Some(else_block)));

    let try_block = ast.block_statement(SPAN, try_stmts);

    // catch(_e) { err = _e; }
    let catch_err_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, ast.str(err_name))));
    let catch_assign = ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(catch_err_target), ast.expression_identifier(SPAN, "_e"));
    let mut catch_stmts = ast.vec();
    catch_stmts.push(ast.statement_expression(SPAN, catch_assign));
    let catch_body = ast.block_statement(SPAN, catch_stmts);
    let catch_pattern = ast.binding_pattern_binding_identifier(SPAN, "_e");
    let catch_clause = ast.catch_clause(SPAN, Some(ast.catch_parameter(SPAN, catch_pattern, NONE)), catch_body);

    stmts.push(ast.statement_try(SPAN, ast.alloc(try_block), Some(ast.alloc(catch_clause)), NONE));
    stmts
}

fn emit_wait<'a>(
    ast: &AstBuilder<'a>,
    names: &[String],
    failed_name: &str,
    kind: &roca::WaitKind,
) -> Vec<Statement<'a>> {
    let mut stmts = Vec::new();

    // Declare result variables
    for name in names {
        let n = ast.str(name);
        let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
        let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, pattern, NONE, None, false);
        let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(declarator), false);
        stmts.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(decl))));
    }
    // Declare failed variable
    let fn_str = ast.str(failed_name);
    let f_pattern = ast.binding_pattern_binding_identifier(SPAN, fn_str);
    let f_declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, f_pattern, NONE, None, false);
    let f_decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(f_declarator), false);
    stmts.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(f_decl))));

    // Build the await expression
    let await_expr = match kind {
        roca::WaitKind::Single(expr) => {
            // await call()
            ast.expression_await(SPAN, build_expr(ast, expr))
        }
        roca::WaitKind::All(exprs) => {
            // await Promise.all([call1(), call2()])
            let mut items = ast.vec();
            for e in exprs {
                items.push(ArrayExpressionElement::from(build_expr(ast, e)));
            }
            let arr = ast.expression_array(SPAN, items);
            let promise_all = Expression::from(ast.member_expression_static(
                SPAN, ast.expression_identifier(SPAN, "Promise"), ast.identifier_name(SPAN, "all"), false,
            ));
            let mut args = ast.vec();
            args.push(Argument::from(arr));
            let call = ast.expression_call(SPAN, promise_all, NONE, args, false);
            ast.expression_await(SPAN, call)
        }
        roca::WaitKind::First(exprs) => {
            // await Promise.race([call1(), call2()])
            let mut items = ast.vec();
            for e in exprs {
                items.push(ArrayExpressionElement::from(build_expr(ast, e)));
            }
            let arr = ast.expression_array(SPAN, items);
            let promise_race = Expression::from(ast.member_expression_static(
                SPAN, ast.expression_identifier(SPAN, "Promise"), ast.identifier_name(SPAN, "race"), false,
            ));
            let mut args = ast.vec();
            args.push(Argument::from(arr));
            let call = ast.expression_call(SPAN, promise_race, NONE, args, false);
            ast.expression_await(SPAN, call)
        }
    };

    // try { result = await ...; } catch(e) { failed = e; }
    let mut try_stmts = ast.vec();

    if names.len() == 1 {
        // Single result: name = await ...
        let n = ast.str(&names[0]);
        let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, n)));
        let assign = ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), await_expr);
        try_stmts.push(ast.statement_expression(SPAN, assign));
    } else {
        // Multiple results (wait all): const _wait_result = await ...; a = _wait_result[0]; b = _wait_result[1];
        let temp_pattern = ast.binding_pattern_binding_identifier(SPAN, "_wait_result");
        let temp_decl = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, temp_pattern, NONE, Some(await_expr), false);
        let temp = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(temp_decl), false);
        try_stmts.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(temp))));

        for (idx, name) in names.iter().enumerate() {
            let n = ast.str(name);
            let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, n)));
            let index = ast.expression_numeric_literal(SPAN, idx as f64, None, NumberBase::Decimal);
            let access = Expression::from(ast.member_expression_computed(
                SPAN, ast.expression_identifier(SPAN, "_wait_result"), index, false,
            ));
            let assign = ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), access);
            try_stmts.push(ast.statement_expression(SPAN, assign));
        }
    }

    let try_block = ast.block_statement(SPAN, try_stmts);

    // catch(e) { failed = e; }
    let fn2 = ast.str(failed_name);
    let fail_target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(ast.identifier_reference(SPAN, fn2)));
    let fail_assign = ast.expression_assignment(
        SPAN, AssignmentOperator::Assign, AssignmentTarget::from(fail_target),
        ast.expression_identifier(SPAN, "_e"),
    );
    let mut catch_stmts = ast.vec();
    catch_stmts.push(ast.statement_expression(SPAN, fail_assign));
    let catch_body = ast.block_statement(SPAN, catch_stmts);
    let err_pattern = ast.binding_pattern_binding_identifier(SPAN, "_e");
    let catch_clause = ast.catch_clause(SPAN, Some(ast.catch_parameter(SPAN, err_pattern, NONE)), catch_body);

    stmts.push(ast.statement_try(SPAN, ast.alloc(try_block), Some(ast.alloc(catch_clause)), NONE));
    stmts
}

/// Skip is passthrough — emit the bare call with no wrapping.
/// Skip means the call is safe, no error handling needed.
fn is_passthrough(kind: &roca::CrashHandlerKind) -> bool {
    matches!(kind, roca::CrashHandlerKind::Simple(chain) if chain.len() == 1 && matches!(chain[0], roca::CrashStep::Skip))
}

/// Look up a crash handler for a call expression
fn find_crash_handler<'a>(expr: &roca::Expr, crash: Option<&'a roca::CrashBlock>) -> Option<&'a roca::CrashHandler> {
    let crash = crash?;
    let call_name = roca::call_to_name(expr)?;
    crash.handlers.iter().find(|h| h.call == call_name)
}
