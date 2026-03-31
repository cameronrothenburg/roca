//! Statement codegen — translates Roca statements into OXC JS statements.
//! Handles let/const bindings, returns, loops, crash handlers, and control flow.

use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::ast_helpers::{
    ident, string_lit, number_lit, null_lit,
    static_field, assign_expr,
    const_decl, let_decl,
    expr_stmt, block, try_catch, if_stmt,
    console_call, args1, args2,
};
use super::expressions::build_expr;
use super::crash::wrap_with_strategy;
use super::helpers::{make_result, make_error, null};

fn zero_value<'a>(ast: &AstBuilder<'a>, t: &roca::TypeRef) -> Expression<'a> {
    match t {
        roca::TypeRef::String => string_lit(ast, ""),
        roca::TypeRef::Number => number_lit(ast, 0.0),
        roca::TypeRef::Bool => ast.expression_boolean_literal(SPAN, false),
        roca::TypeRef::Nullable(_) => null_lit(ast),
        _ => null_lit(ast),
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
            emit_var_decl(ast, name, value, VariableDeclarationKind::Const, crash, returns_err)
        }
        roca::Stmt::Let { name, value, .. } => {
            emit_var_decl(ast, name, value, VariableDeclarationKind::Let, crash, returns_err)
        }
        roca::Stmt::LetResult { name, err_name, value } => {
            if let Some((cast_type, input)) = extract_cast_input(value) {
                let mut result = emit_safe_cast(ast, name, err_name, &cast_type, input);
                // Apply crash strategy to safe cast (e.g., fallback on error)
                let handler = find_crash_handler(value, crash);
                let terminal = handler.and_then(|h| match &h.strategy {
                    roca::CrashHandlerKind::Simple(chain) => chain.last(),
                    _ => None,
                });
                if let Some(terminal) = terminal {
                    match terminal {
                        roca::CrashStep::Fallback(fallback_expr) => {
                            let err_check = ident(ast, err_name);
                            let fb_val = if matches!(fallback_expr, roca::Expr::Closure { .. }) {
                                let closure = build_expr(ast, fallback_expr);
                                let args = args1(ast, ident(ast, err_name));
                                ast.expression_call(SPAN, closure, NONE, args, false)
                            } else {
                                build_expr(ast, fallback_expr)
                            };
                            let assign = assign_expr(ast, name, fb_val);
                            let mut then = ast.vec();
                            then.push(expr_stmt(ast, assign));
                            let consequent = block(ast, then);
                            result.push(if_stmt(ast, err_check, consequent, None));
                        }
                        roca::CrashStep::Halt if returns_err => {
                            let err_check = ident(ast, err_name);
                            let zero = zero_value(ast, return_type);
                            let propagate_err = ident(ast, err_name);
                            let ret = make_result(ast, zero, propagate_err);
                            let mut then = ast.vec();
                            then.push(ast.statement_return(SPAN, Some(ret)));
                            let consequent = block(ast, then);
                            result.push(if_stmt(ast, err_check, consequent, None));
                        }
                        _ => {} // skip, etc. — no action needed
                    }
                }
                return result;
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
                            let err_check = ident(ast, err_name);
                            let zero = zero_value(ast, return_type);
                            let propagate_err = ident(ast, err_name);
                            let ret = make_result(ast, zero, propagate_err);
                            let mut then = ast.vec();
                            then.push(ast.statement_return(SPAN, Some(ret)));
                            let consequent = block(ast, then);
                            result.push(if_stmt(ast, err_check, consequent, None));
                        }
                        roca::CrashStep::Fallback(fallback_expr) => {
                            let err_check = ident(ast, err_name);
                            let fb_val = if matches!(fallback_expr, roca::Expr::Closure { .. }) {
                                let closure = build_expr(ast, fallback_expr);
                                let args = args1(ast, ident(ast, err_name));
                                ast.expression_call(SPAN, closure, NONE, args, false)
                            } else {
                                build_expr(ast, fallback_expr)
                            };
                            let assign = assign_expr(ast, name, fb_val);
                            let mut then = ast.vec();
                            then.push(expr_stmt(ast, assign));
                            let consequent = block(ast, then);
                            result.push(if_stmt(ast, err_check, consequent, None));
                        }
                        roca::CrashStep::Panic => {
                            let err_check = ident(ast, err_name);
                            let mut panic_stmts = ast.vec();
                            let console_err = console_call(ast, "error", args2(ast, string_lit(ast, "PANIC:"), ident(ast, err_name)));
                            panic_stmts.push(expr_stmt(ast, console_err));
                            let exit_call = ast.expression_call(SPAN,
                                static_field(ast, "process", "exit"),
                                NONE, args1(ast, number_lit(ast, 1.0)), false);
                            panic_stmts.push(expr_stmt(ast, exit_call));
                            let consequent = block(ast, panic_stmts);
                            result.push(if_stmt(ast, err_check, consequent, None));
                        }
                        roca::CrashStep::Skip => {}
                        _ => {}
                    }
            }

            result
        }
        roca::Stmt::Return(expr) => {
            // Crash-handled calls must be unwrapped before returning to avoid double-wrapping.
            // The crash handler extracts .value from the error tuple — the return just needs
            // to wrap that extracted value in { value, err: null } if returns_err.
            if let Some(handler) = find_crash_handler(expr, crash) {
                // Skip on a non-error-returning call is a no-op — just return the raw value.
                if is_passthrough(&handler.strategy) {
                    let val = build_expr(ast, expr);
                    if returns_err {
                        let ret = make_result(ast, val, null(ast));
                        return vec![ast.statement_return(SPAN, Some(ret))];
                    } else {
                        return vec![ast.statement_return(SPAN, Some(val))];
                    }
                }
                let call_expr = build_expr(ast, expr);
                let tmp_name = "_ret";
                let mut stmts = wrap_with_strategy(ast, call_expr, tmp_name, &handler.strategy, expr, returns_err);
                let ret_val = ident(ast, tmp_name);
                if returns_err {
                    stmts.push(ast.statement_return(SPAN, Some(make_result(ast, ret_val, null(ast)))));
                } else {
                    stmts.push(ast.statement_return(SPAN, Some(ret_val)));
                }
                return stmts;
            }

            let val = build_expr(ast, expr);
            // If expression is a match with err arms, it already produces tuples — don't double-wrap
            let already_tupled = if let roca::Expr::Match { arms, .. } = expr {
                super::shapes::match_has_err_arms(arms)
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
                string_lit(ast, &default_msg)
            };
            let err = make_error(ast, err_name, msg_expr);
            let zero = zero_value(ast, return_type);
            let ret = make_result(ast, zero, err);
            vec![ast.statement_return(SPAN, Some(ret))]
        }
        roca::Stmt::Assign { name, value } => {
            let val = build_expr(ast, value);
            let assign = assign_expr(ast, name, val);
            vec![expr_stmt(ast, assign)]
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
            vec![expr_stmt(ast, assign)]
        }
        roca::Stmt::Expr(expr) => {
            if let Some(handler) = find_crash_handler(expr, crash) {
                if !is_passthrough(&handler.strategy) {
                    let call_expr = build_expr(ast, expr);
                    let id = CRASH_COUNTER.fetch_add(1, Ordering::Relaxed);
                    let var_name = format!("_r{}", id);
                    return wrap_with_strategy(ast, call_expr, &var_name, &handler.strategy, expr, returns_err);
                }
            }
            let val = build_expr(ast, expr);
            vec![expr_stmt(ast, val)]
        }
        roca::Stmt::If { condition, then_body, else_body } => {
            let test = build_expr(ast, condition);
            let mut then_stmts = ast.vec();
            for s in then_body {
                for emitted in build_stmt(ast, s, returns_err, return_type, errors_decl, crash) {
                    then_stmts.push(emitted);
                }
            }
            let consequent = block(ast, then_stmts);

            let alternate = else_body.as_ref().map(|body| {
                let mut stmts = ast.vec();
                for s in body {
                    for emitted in build_stmt(ast, s, returns_err, return_type, errors_decl, crash) {
                        stmts.push(emitted);
                    }
                }
                block(ast, stmts)
            });

            vec![if_stmt(ast, test, consequent, alternate)]
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
            let body_stmt = block(ast, stmts);
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
            let body_stmt = block(ast, stmts);
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
    returns_err: bool,
) -> Vec<Statement<'a>> {
    if let Some(handler) = find_crash_handler(value, crash) {
        if !is_passthrough(&handler.strategy) {
            let call_expr = build_expr(ast, value);
            return wrap_with_strategy(ast, call_expr, name, &handler.strategy, value, returns_err);
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
    stmts.push(let_decl(ast, name));
    stmts.push(let_decl(ast, err_name));

    let mut try_stmts = ast.vec();

    // const _input = <input expr>; — build once, reference twice
    let input_val = build_expr(ast, input_expr);
    try_stmts.push(const_decl(ast, "_input", input_val));

    // const _raw = Type(_input)
    let js_type = if cast_type == "Bool" { "Boolean" } else { cast_type };
    let type_name = ast.str(js_type);
    let cast_call = ast.expression_call(SPAN, ast.expression_identifier(SPAN, type_name), NONE, args1(ast, ident(ast, "_input")), false);
    try_stmts.push(const_decl(ast, "_raw", cast_call));

    // null check: _input === null || _input === undefined
    let null_check = ast.expression_binary(SPAN, ident(ast, "_input"), BinaryOperator::StrictEquality, null_lit(ast));
    let undef_check = ast.expression_binary(SPAN, ident(ast, "_input"), BinaryOperator::StrictEquality, ident(ast, "undefined"));
    let mut condition = ast.expression_logical(SPAN, null_check, LogicalOperator::Or, undef_check);

    // Type-specific check
    if cast_type == "Number" {
        // Also check isNaN
        let nan_check = ast.expression_call(
            SPAN,
            static_field(ast, "Number", "isNaN"),
            NONE, args1(ast, ident(ast, "_raw")), false,
        );
        condition = ast.expression_logical(SPAN, condition, LogicalOperator::Or, nan_check);
    }

    // if (condition) { err = new Error("invalid_cast"); } else { value = _raw; }
    let error_msg = ast.str(&format!("invalid_{}", cast_type.to_lowercase()));
    let new_error = ast.expression_new(SPAN, ident(ast, "Error"), NONE, args1(ast, ast.expression_string_literal(SPAN, error_msg, None)));
    let err_assign = assign_expr(ast, err_name, new_error);
    let mut then_stmts = ast.vec();
    then_stmts.push(expr_stmt(ast, err_assign));
    let then_block = block(ast, then_stmts);

    let val_assign = assign_expr(ast, name, ident(ast, "_raw"));
    let mut else_stmts = ast.vec();
    else_stmts.push(expr_stmt(ast, val_assign));
    let else_block = block(ast, else_stmts);

    try_stmts.push(if_stmt(ast, condition, then_block, Some(else_block)));

    // catch(_e) { err = _e; }
    let catch_assign = assign_expr(ast, err_name, ident(ast, "_e"));
    let mut catch_stmts = ast.vec();
    catch_stmts.push(expr_stmt(ast, catch_assign));

    stmts.push(try_catch(ast, try_stmts, "_e", catch_stmts));
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
        stmts.push(let_decl(ast, name));
    }
    // Declare failed variable
    stmts.push(let_decl(ast, failed_name));

    // Build the await expression
    let await_expression = match kind {
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
            let promise_all = static_field(ast, "Promise", "all");
            let call = ast.expression_call(SPAN, promise_all, NONE, args1(ast, arr), false);
            ast.expression_await(SPAN, call)
        }
        roca::WaitKind::First(exprs) => {
            // await Promise.race([call1(), call2()])
            let mut items = ast.vec();
            for e in exprs {
                items.push(ArrayExpressionElement::from(build_expr(ast, e)));
            }
            let arr = ast.expression_array(SPAN, items);
            let promise_race = static_field(ast, "Promise", "race");
            let call = ast.expression_call(SPAN, promise_race, NONE, args1(ast, arr), false);
            ast.expression_await(SPAN, call)
        }
    };

    // try { result = await ...; } catch(e) { failed = e; }
    let mut try_stmts = ast.vec();

    if names.len() == 1 {
        // Single result: name = await ...
        let assign = assign_expr(ast, &names[0], await_expression);
        try_stmts.push(expr_stmt(ast, assign));
    } else {
        // Multiple results (wait all): const _wait_result = await ...; a = _wait_result[0]; b = _wait_result[1];
        try_stmts.push(const_decl(ast, "_wait_result", await_expression));

        for (idx, name) in names.iter().enumerate() {
            let index = number_lit(ast, idx as f64);
            let access = Expression::from(ast.member_expression_computed(
                SPAN, ident(ast, "_wait_result"), index, false,
            ));
            let assign = assign_expr(ast, name, access);
            try_stmts.push(expr_stmt(ast, assign));
        }
    }

    // catch(e) { failed = e; }
    let fail_assign = assign_expr(ast, failed_name, ident(ast, "_e"));
    let mut catch_stmts = ast.vec();
    catch_stmts.push(expr_stmt(ast, fail_assign));

    stmts.push(try_catch(ast, try_stmts, "_e", catch_stmts));
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
