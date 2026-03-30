//! Function codegen — builds OXC `Function` nodes from Roca function definitions.
//! Handles params, async detection, and body statement emission.

use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::ast_helpers::{param, formal_params, function_body};
use super::statements::build_stmt;

pub(crate) fn build_function<'a>(ast: &AstBuilder<'a>, f: &roca::FnDef) -> Function<'a> {
    let n = ast.str(&f.name);
    let id = ast.binding_identifier(SPAN, n);

    let mut params_list = ast.vec();
    for p in &f.params {
        params_list.push(param(ast, &p.name));
    }
    let params = formal_params(ast, params_list);

    let mut stmts = ast.vec();
    for s in &f.body {
        for emitted in build_stmt(ast, s, f.returns_err, &f.return_type, &f.errors, f.crash.as_ref()) {
            stmts.push(emitted);
        }
    }
    let body = function_body(ast, stmts);

    let is_async = body_has_wait(&f.body);

    ast.function(
        SPAN,
        FunctionType::FunctionDeclaration,
        Some(id),
        false,    // generator
        is_async, // async — auto-detected from wait statements
        false,    // declare
        NONE, NONE, params, NONE, Some(body),
    )
}

pub(crate) fn body_has_wait(stmts: &[roca::Stmt]) -> bool {
    stmts.iter().any(|s| match s {
        roca::Stmt::Wait { .. } => true,
        roca::Stmt::Const { value, .. } | roca::Stmt::Let { value, .. } | roca::Stmt::Return(value) | roca::Stmt::Expr(value) => {
            expr_has_await(value)
        }
        roca::Stmt::If { condition, then_body, else_body } => {
            expr_has_await(condition) || body_has_wait(then_body) || else_body.as_ref().map_or(false, |b| body_has_wait(b))
        }
        roca::Stmt::For { iter, body, .. } => expr_has_await(iter) || body_has_wait(body),
        roca::Stmt::While { condition, body } => expr_has_await(condition) || body_has_wait(body),
        _ => false,
    })
}

fn expr_has_await(expr: &roca::Expr) -> bool {
    match expr {
        roca::Expr::Await(_) => true,
        roca::Expr::Call { target, args } => expr_has_await(target) || args.iter().any(|a| expr_has_await(a)),
        roca::Expr::FieldAccess { target, .. } => expr_has_await(target),
        roca::Expr::BinOp { left, right, .. } => expr_has_await(left) || expr_has_await(right),
        _ => false,
    }
}
