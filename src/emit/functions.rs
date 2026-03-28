use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::statements::build_stmt;

/// Build a standalone function declaration
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

    ast.function(
        SPAN,
        FunctionType::FunctionDeclaration,
        Some(id),
        false, false, false,
        NONE, NONE, formal_params, NONE, Some(body),
    )
}
