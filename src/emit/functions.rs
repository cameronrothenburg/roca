use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::statements::build_stmt;

pub(crate) fn build_function<'a>(ast: &AstBuilder<'a>, f: &roca::FnDef) -> Function<'a> {
    let n = ast.str(&f.name);
    let id = ast.binding_identifier(SPAN, n);

    let mut params_list = ast.vec();
    for p in &f.params {
        let pn = ast.str(&p.name);
        let pattern = ast.binding_pattern_binding_identifier(SPAN, pn);
        params_list.push(ast.plain_formal_parameter(SPAN, pattern));
    }
    let formal_params = ast.formal_parameters(SPAN, FormalParameterKind::FormalParameter, params_list, NONE);

    let mut stmts = ast.vec();
    for s in &f.body {
        for emitted in build_stmt(ast, s, f.returns_err, f.crash.as_ref()) {
            stmts.push(emitted);
        }
    }
    let body = ast.function_body(SPAN, ast.vec(), stmts);

    let is_async = body_has_wait(&f.body);

    ast.function(
        SPAN,
        FunctionType::FunctionDeclaration,
        Some(id),
        false,    // generator
        is_async, // async — auto-detected from wait statements
        false,    // declare
        NONE, NONE, formal_params, NONE, Some(body),
    )
}

fn body_has_wait(stmts: &[roca::Stmt]) -> bool {
    stmts.iter().any(|s| match s {
        roca::Stmt::Wait { .. } => true,
        roca::Stmt::If { then_body, else_body, .. } => {
            body_has_wait(then_body) || else_body.as_ref().map_or(false, |b| body_has_wait(b))
        }
        roca::Stmt::For { body, .. } => body_has_wait(body),
        _ => false,
    })
}
