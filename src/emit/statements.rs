use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::expressions::build_expr;
use super::crash::wrap_with_strategy;
use super::helpers::{make_tuple, make_error, null};

/// Build a statement, optionally wrapping calls with crash handlers
pub(crate) fn build_stmt<'a>(
    ast: &AstBuilder<'a>,
    stmt: &roca::Stmt,
    returns_err: bool,
    crash: Option<&roca::CrashBlock>,
) -> Vec<Statement<'a>> {
    match stmt {
        roca::Stmt::Const { name, value, .. } => {
            if let Some(handler) = find_crash_handler(value, crash) {
                if !is_halt(&handler.strategy) {
                    let call_expr = build_expr(ast, value);
                    return wrap_with_strategy(ast, call_expr, name, &handler.strategy);
                }
            }
            let n = ast.str(name);
            let init = build_expr(ast, value);
            let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
            let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, pattern, NONE, Some(init), false);
            let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
            vec![Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))]
        }
        roca::Stmt::Let { name, value, .. } => {
            if let Some(handler) = find_crash_handler(value, crash) {
                if !is_halt(&handler.strategy) {
                    let call_expr = build_expr(ast, value);
                    return wrap_with_strategy(ast, call_expr, name, &handler.strategy);
                }
            }
            let n = ast.str(name);
            let init = build_expr(ast, value);
            let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
            let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, pattern, NONE, Some(init), false);
            let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(declarator), false);
            vec![Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))]
        }
        roca::Stmt::LetResult { name, err_name, value } => {
            let n = ast.str(name);
            let e = ast.str(err_name);
            let mut elements = ast.vec();
            let bind1 = ast.binding_pattern_binding_identifier(SPAN, n);
            elements.push(Some(bind1));
            let bind2 = ast.binding_pattern_binding_identifier(SPAN, e);
            elements.push(Some(bind2));
            let array_pattern = ast.binding_pattern_array_pattern(SPAN, elements, NONE);
            let init = build_expr(ast, value);
            let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, array_pattern, NONE, Some(init), false);
            let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
            vec![Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))]
        }
        roca::Stmt::Return(expr) => {
            let val = build_expr(ast, expr);
            if returns_err {
                let ret = make_tuple(ast, val, null(ast));
                vec![ast.statement_return(SPAN, Some(ret))]
            } else {
                vec![ast.statement_return(SPAN, Some(val))]
            }
        }
        roca::Stmt::ReturnErr(err_name) => {
            let err = make_error(ast, err_name);
            let ret = make_tuple(ast, null(ast), err);
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
        roca::Stmt::Expr(expr) => {
            if let Some(handler) = find_crash_handler(expr, crash) {
                if !is_halt(&handler.strategy) {
                    let call_expr = build_expr(ast, expr);
                    let var_name = "_result";
                    return wrap_with_strategy(ast, call_expr, var_name, &handler.strategy);
                }
            }
            let val = build_expr(ast, expr);
            vec![ast.statement_expression(SPAN, val)]
        }
        roca::Stmt::If { condition, then_body, else_body } => {
            let test = build_expr(ast, condition);
            let mut then_stmts = ast.vec();
            for s in then_body {
                for emitted in build_stmt(ast, s, returns_err, crash) {
                    then_stmts.push(emitted);
                }
            }
            let consequent = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, then_stmts)));

            let alternate = else_body.as_ref().map(|body| {
                let mut stmts = ast.vec();
                for s in body {
                    for emitted in build_stmt(ast, s, returns_err, crash) {
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
                for emitted in build_stmt(ast, s, returns_err, crash) {
                    stmts.push(emitted);
                }
            }
            let body_stmt = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)));
            vec![ast.statement_for_of(SPAN, false, left, right, body_stmt)]
        }
    }
}

/// Extract the dotted call name from an expression (e.g. "http.get", "Email.validate", "name.trim")
fn expr_call_name(expr: &roca::Expr) -> Option<String> {
    match expr {
        roca::Expr::Call { target, .. } => target_to_name(target),
        _ => None,
    }
}

fn target_to_name(expr: &roca::Expr) -> Option<String> {
    match expr {
        roca::Expr::Ident(name) => Some(name.clone()),
        roca::Expr::FieldAccess { target, field } => {
            let parent = match target.as_ref() {
                roca::Expr::Ident(name) => Some(name.clone()),
                roca::Expr::SelfRef => Some("self".to_string()),
                roca::Expr::FieldAccess { .. } => target_to_name(target),
                _ => None,
            };
            parent.map(|p| format!("{}.{}", p, field))
        }
        _ => None,
    }
}

fn is_halt(kind: &roca::CrashHandlerKind) -> bool {
    matches!(kind, roca::CrashHandlerKind::Simple(roca::CrashStrategy::Halt))
}

/// Look up a crash handler for a call expression
fn find_crash_handler<'a>(expr: &roca::Expr, crash: Option<&'a roca::CrashBlock>) -> Option<&'a roca::CrashHandler> {
    let crash = crash?;
    let call_name = expr_call_name(expr)?;
    crash.handlers.iter().find(|h| h.call == call_name)
}
