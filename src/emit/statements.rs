use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::expressions::build_expr;
use super::helpers::{make_tuple, make_error, null};

pub(crate) fn build_stmt<'a>(ast: &AstBuilder<'a>, stmt: &roca::Stmt, returns_err: bool) -> Statement<'a> {
    match stmt {
        roca::Stmt::Const { name, value, .. } => {
            let n = ast.str(name);
            let init = build_expr(ast, value);
            let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
            let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, pattern, NONE, Some(init), false);
            let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
            Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))
        }
        roca::Stmt::Let { name, value, .. } => {
            let n = ast.str(name);
            let init = build_expr(ast, value);
            let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
            let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Let, pattern, NONE, Some(init), false);
            let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Let, ast.vec1(declarator), false);
            Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))
        }
        roca::Stmt::LetResult { name, err_name, value } => {
            // const [name, err_name] = value;
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
            Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))
        }
        roca::Stmt::Return(expr) => {
            let val = build_expr(ast, expr);
            if returns_err {
                let ret = make_tuple(ast, val, null(ast));
                ast.statement_return(SPAN, Some(ret))
            } else {
                ast.statement_return(SPAN, Some(val))
            }
        }
        roca::Stmt::ReturnErr(err_name) => {
            let err = make_error(ast, err_name);
            let ret = make_tuple(ast, null(ast), err);
            ast.statement_return(SPAN, Some(ret))
        }
        roca::Stmt::Assign { name, value } => {
            let n = ast.str(name);
            let id_ref = ast.identifier_reference(SPAN, n);
            let target = SimpleAssignmentTarget::AssignmentTargetIdentifier(ast.alloc(id_ref));
            let val = build_expr(ast, value);
            let assign = ast.expression_assignment(
                SPAN, AssignmentOperator::Assign, AssignmentTarget::from(target), val,
            );
            ast.statement_expression(SPAN, assign)
        }
        roca::Stmt::Expr(expr) => {
            let val = build_expr(ast, expr);
            ast.statement_expression(SPAN, val)
        }
        roca::Stmt::If { condition, then_body, else_body } => {
            let test = build_expr(ast, condition);
            let mut then_stmts = ast.vec();
            for s in then_body {
                then_stmts.push(build_stmt(ast, s, returns_err));
            }
            let consequent = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, then_stmts)));

            let alternate = else_body.as_ref().map(|body| {
                let mut stmts = ast.vec();
                for s in body {
                    stmts.push(build_stmt(ast, s, returns_err));
                }
                Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)))
            });

            ast.statement_if(SPAN, test, consequent, alternate)
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
                stmts.push(build_stmt(ast, s, returns_err));
            }
            let body_stmt = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)));
            ast.statement_for_of(SPAN, false, left, right, body_stmt)
        }
    }
}
