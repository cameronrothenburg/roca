//! Function codegen — builds OXC `Function` nodes from Roca function definitions.
//! Handles params, async detection, and body statement emission.

use roca_ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::ast_helpers::{param, formal_params, function_body, binary, if_stmt, throw_stmt, number_lit, string_lit, field_access, block, arg, ident};
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

    // Emit parameter constraint guards at function entry
    for p in &f.params {
        emit_param_guards(ast, p, &mut stmts);
    }

    for s in &f.body {
        for emitted in build_stmt(ast, s, f.returns_err, &f.return_type, &f.errors, f.crash.as_ref()) {
            stmts.push(emitted);
        }
    }
    let body = function_body(ast, stmts);

    let is_async = body_has_wait(&f.body) || crash_has_delay(&f.crash);

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

/// Check if a crash block contains retry with a delay, which emits await.
fn crash_has_delay(crash: &Option<roca::CrashBlock>) -> bool {
    let crash = match crash {
        Some(c) => c,
        None => return false,
    };
    for handler in &crash.handlers {
        let chains: Vec<&[roca::CrashStep]> = match &handler.strategy {
            roca::CrashHandlerKind::Simple(chain) => vec![chain],
            roca::CrashHandlerKind::Detailed { arms, default } => {
                let mut v: Vec<&[roca::CrashStep]> = arms.iter().map(|a| a.chain.as_slice()).collect();
                if let Some(d) = default { v.push(d); }
                v
            }
        };
        for chain in chains {
            if chain.iter().any(|s| matches!(s, roca::CrashStep::Retry { delay_ms, .. } if *delay_ms > 0)) {
                return true;
            }
        }
    }
    false
}

fn emit_param_guards<'a>(
    ast: &AstBuilder<'a>,
    p: &roca::Param,
    stmts: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) {
    if p.constraints.is_empty() { return; }
    let is_string = matches!(p.type_ref, roca::TypeRef::String);
    emit_constraint_guards(ast, &p.name, is_string, &p.constraints, &|a| ident(a, &p.name), stmts);
}

/// Shared constraint guard emission for JS — used by both param and struct field constraints.
/// `make_val` returns a fresh expression referencing the value to check (e.g., `ident("x")` or `init.x`).
pub(crate) fn emit_constraint_guards<'a>(
    ast: &AstBuilder<'a>,
    name: &str,
    is_string: bool,
    constraints: &[roca::Constraint],
    make_val: &dyn Fn(&AstBuilder<'a>) -> Expression<'a>,
    stmts: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) {
    for constraint in constraints {
        let (test, msg) = match constraint {
            roca::Constraint::Min(n) if !is_string => {
                (binary(ast, make_val(ast), BinaryOperator::LessThan, number_lit(ast, *n)),
                 format!("{}: must be >= {}", name, n))
            }
            roca::Constraint::Max(n) if !is_string => {
                (binary(ast, make_val(ast), BinaryOperator::GreaterThan, number_lit(ast, *n)),
                 format!("{}: must be <= {}", name, n))
            }
            roca::Constraint::Min(n) | roca::Constraint::MinLen(n) if is_string => {
                let len = field_access(ast, make_val(ast), ast.str("length"));
                (binary(ast, len, BinaryOperator::LessThan, number_lit(ast, *n)),
                 format!("{}: min length {}", name, n))
            }
            roca::Constraint::Max(n) | roca::Constraint::MaxLen(n) if is_string => {
                let len = field_access(ast, make_val(ast), ast.str("length"));
                (binary(ast, len, BinaryOperator::GreaterThan, number_lit(ast, *n)),
                 format!("{}: max length {}", name, n))
            }
            roca::Constraint::Contains(s) => {
                let mut call_args = ast.vec();
                call_args.push(arg(string_lit(ast, s)));
                let includes = ast.expression_call(
                    SPAN, field_access(ast, make_val(ast), ast.str("includes")), NONE, call_args, false,
                );
                let not_includes = ast.expression_unary(SPAN, UnaryOperator::LogicalNot, includes);
                (not_includes, format!("{}: must contain \"{}\"", name, s))
            }
            roca::Constraint::Pattern(p_str) => {
                let mut re_args = ast.vec();
                re_args.push(arg(string_lit(ast, p_str)));
                let regex = ast.expression_new(SPAN, ident(ast, "RegExp"), NONE, re_args);
                let mut test_args = ast.vec();
                test_args.push(arg(make_val(ast)));
                let test_call = ast.expression_call(
                    SPAN, field_access(ast, regex, ast.str("test")), NONE, test_args, false,
                );
                let not_match = ast.expression_unary(SPAN, UnaryOperator::LogicalNot, test_call);
                (not_match, format!("{}: must match pattern /{}/", name, p_str))
            }
            roca::Constraint::Default(_) => continue,
            _ => continue,
        };

        let mut err_args = ast.vec();
        err_args.push(arg(string_lit(ast, &msg)));
        let err = ast.expression_new(SPAN, ident(ast, "Error"), NONE, err_args);
        let throw = throw_stmt(ast, err);
        let mut body = ast.vec();
        body.push(throw);
        stmts.push(if_stmt(ast, test, block(ast, body), None));
    }
}

pub(crate) fn expr_has_await(expr: &roca::Expr) -> bool {
    match expr {
        roca::Expr::Await(_) => true,
        roca::Expr::Call { target, args } => expr_has_await(target) || args.iter().any(|a| expr_has_await(a)),
        roca::Expr::FieldAccess { target, .. } => expr_has_await(target),
        roca::Expr::BinOp { left, right, .. } => expr_has_await(left) || expr_has_await(right),
        _ => false,
    }
}
