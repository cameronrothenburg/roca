//! Expression codegen — translates Roca expressions into OXC JS expressions.
//! Handles literals, match arms, field access, calls, and error-result wrapping.

use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::ast_helpers::{
    ident, string_lit, number_lit, bool_lit, null_lit,
    static_field, unary_not, binary,
    prop, object_expr, param,
    function_body, expr_stmt, arg,
};
use super::helpers::{make_error_simple, make_result, null};

/// Check if any match arm references err.X
pub(crate) fn match_has_err_arms(arms: &[roca::MatchArm]) -> bool {
    arms.iter().any(|arm| is_err_ref(&arm.value))
}

fn is_err_ref(expr: &roca::Expr) -> bool {
    if let roca::Expr::FieldAccess { target, .. } = expr {
        if let roca::Expr::Ident(name) = target.as_ref() {
            return name == "err";
        }
    }
    false
}

/// Build a match arm value — handles err.X pattern as [null, new Error("X")]
/// When mixed (some arms return errors, some don't), wraps non-error values as [value, null]
fn build_match_arm_value<'a>(ast: &AstBuilder<'a>, expr: &roca::Expr, mixed: bool) -> Expression<'a> {
    if let roca::Expr::FieldAccess { target, field } = expr {
        if let roca::Expr::Ident(name) = target.as_ref() {
            if name == "err" {
                return make_result(ast, null(ast), make_error_simple(ast, field));
            }
        }
    }
    let val = build_expr(ast, expr);
    if mixed {
        // Wrap non-error values in [value, null] for consistency
        make_result(ast, val, null(ast))
    } else {
        val
    }
}

pub(crate) fn build_expr<'a>(ast: &AstBuilder<'a>, expr: &roca::Expr) -> Expression<'a> {
    match expr {
        roca::Expr::String(s) => string_lit(ast, s),
        roca::Expr::Number(n) => number_lit(ast, *n),
        roca::Expr::Bool(b) => bool_lit(ast, *b),
        roca::Expr::Ident(name) => {
            if name == "Ok" {
                null_lit(ast)
            } else {
                ident(ast, name)
            }
        }
        roca::Expr::Not(inner) => {
            let expr = build_expr(ast, inner);
            unary_not(ast, expr)
        }
        roca::Expr::Closure { params, body } => {
            let mut param_list = ast.vec();
            for p in params {
                param_list.push(param(ast, p));
            }
            let formal_params = ast.formal_parameters(SPAN, FormalParameterKind::ArrowFormalParameters, param_list, NONE);
            let body_expr = build_expr(ast, body);
            let body_stmt = expr_stmt(ast, body_expr);
            let mut stmts = ast.vec();
            stmts.push(body_stmt);
            let fn_body = function_body(ast, stmts);
            ast.expression_arrow_function(SPAN, true, false, NONE, formal_params, NONE, fn_body)
        }
        roca::Expr::Null => null_lit(ast),
        roca::Expr::SelfRef => ast.expression_this(SPAN),
        roca::Expr::BinOp { left, op, right } => {
            let l = build_expr(ast, left);
            let r = build_expr(ast, right);
            match op {
                roca::BinOp::And => ast.expression_logical(SPAN, l, LogicalOperator::And, r),
                roca::BinOp::Or => ast.expression_logical(SPAN, l, LogicalOperator::Or, r),
                _ => {
                    let js_op = match op {
                        roca::BinOp::Add => BinaryOperator::Addition,
                        roca::BinOp::Sub => BinaryOperator::Subtraction,
                        roca::BinOp::Mul => BinaryOperator::Multiplication,
                        roca::BinOp::Div => BinaryOperator::Division,
                        roca::BinOp::Eq => BinaryOperator::StrictEquality,
                        roca::BinOp::Neq => BinaryOperator::StrictInequality,
                        roca::BinOp::Lt => BinaryOperator::LessThan,
                        roca::BinOp::Gt => BinaryOperator::GreaterThan,
                        roca::BinOp::Lte => BinaryOperator::LessEqualThan,
                        roca::BinOp::Gte => BinaryOperator::GreaterEqualThan,
                        _ => unreachable!(),
                    };
                    binary(ast, l, js_op, r)
                }
            }
        }
        roca::Expr::Call { target, args } => {
            // Map log/error/warn to console.log/error/warn
            if let roca::Expr::Ident(name) = target.as_ref() {
                let console_method = match name.as_str() {
                    "log" => Some("log"),
                    "error" => Some("error"),
                    "warn" => Some("warn"),
                    _ => None,
                };
                if let Some(method) = console_method {
                    let callee = static_field(ast, "console", method);
                    let mut oxc_args = ast.vec();
                    for a in args {
                        oxc_args.push(arg(build_expr(ast, a)));
                    }
                    return ast.expression_call(SPAN, callee, NONE, oxc_args, false);
                }
            }
            // Uppercase calls: primitives = type conversion (no new), structs = constructor (new)
            if let roca::Expr::Ident(name) = target.as_ref() {
                if name.chars().next().map_or(false, |c| c.is_uppercase()) {
                    let callee = ident(ast, name);
                    let mut oxc_args = ast.vec();
                    for a in args {
                        oxc_args.push(arg(build_expr(ast, a)));
                    }
                    // Primitive conversions: String(x), Number(x), Bool(x) — no new
                    if matches!(name.as_str(), "String" | "Number") {
                        return ast.expression_call(SPAN, callee, NONE, oxc_args, false);
                    }
                    // Bool -> Boolean in JS
                    if name == "Bool" {
                        let js_callee = ident(ast, "Boolean");
                        return ast.expression_call(SPAN, js_callee, NONE, oxc_args, false);
                    }
                    return ast.expression_new(SPAN, callee, NONE, oxc_args);
                }
            }
            let callee = build_expr(ast, target);
            let mut oxc_args = ast.vec();
            for a in args {
                oxc_args.push(arg(build_expr(ast, a)));
            }
            ast.expression_call(SPAN, callee, NONE, oxc_args, false)
        }
        roca::Expr::FieldAccess { target, field } => {
            let obj = build_expr(ast, target);
            let f = ast.str(field);
            Expression::from(ast.member_expression_static(SPAN, obj, ast.identifier_name(SPAN, f), false))
        }
        roca::Expr::StructLit { name, fields } => {
            let mut props_list = ast.vec();
            for (key, val) in fields {
                let value = build_expr(ast, val);
                props_list.push(prop(ast, key, value));
            }
            let obj = object_expr(ast, props_list);
            // new Name({ fields })
            let callee = ident(ast, name);
            let mut call_args = ast.vec();
            call_args.push(arg(obj));
            ast.expression_new(SPAN, callee, NONE, call_args)
        }
        roca::Expr::Array(elements) => {
            let mut items = ast.vec();
            for el in elements {
                items.push(ArrayExpressionElement::from(build_expr(ast, el)));
            }
            ast.expression_array(SPAN, items)
        }
        roca::Expr::StringInterp(parts) => {
            // Emit as JS template literal: `lit${expr}lit${expr}lit`
            let mut quasis = ast.vec();
            let mut expressions = ast.vec();

            for (i, part) in parts.iter().enumerate() {
                match part {
                    roca::StringPart::Literal(s) => {
                        let raw: oxc_span::Atom = ast.str(s).into();
                        let tail = i == parts.len() - 1;
                        let value = TemplateElementValue { raw, cooked: None };
                        quasis.push(ast.template_element(SPAN, value, tail, false));
                    }
                    roca::StringPart::Expr(expr) => {
                        if quasis.is_empty() {
                            let raw: oxc_span::Atom = ast.str("").into();
                            let value = TemplateElementValue { raw, cooked: None };
                            quasis.push(ast.template_element(SPAN, value, false, false));
                        }
                        expressions.push(build_expr(ast, expr));
                    }
                }
            }

            if quasis.len() <= expressions.len() {
                let raw: oxc_span::Atom = ast.str("").into();
                let value = TemplateElementValue { raw, cooked: None };
                quasis.push(ast.template_element(SPAN, value, true, false));
            }

            ast.expression_template_literal(SPAN, quasis, expressions)
        }
        roca::Expr::Index { target, index } => {
            let obj = build_expr(ast, target);
            let idx = build_expr(ast, index);
            Expression::from(ast.member_expression_computed(SPAN, obj, idx, false))
        }
        roca::Expr::Match { value, arms } => {
            // Emit as nested ternaries: val === p1 ? r1 : val === p2 ? r2 : default
            let mixed = match_has_err_arms(arms);
            let mut result: Option<Expression<'a>> = None;

            for arm in arms.iter().rev() {
                let arm_value = build_match_arm_value(ast, &arm.value, mixed);
                match &arm.pattern {
                    None => {
                        // Default arm
                        result = Some(arm_value);
                    }
                    Some(pattern) => {
                        let val = build_expr(ast, value);
                        let pat = build_expr(ast, pattern);
                        let test = binary(ast, val, BinaryOperator::StrictEquality, pat);
                        let alternate = result.unwrap_or_else(|| ident(ast, "undefined"));
                        result = Some(ast.expression_conditional(SPAN, test, arm_value, alternate));
                    }
                }
            }

            result.unwrap_or_else(|| ident(ast, "undefined"))
        }
        roca::Expr::Await(inner) => {
            let expr = build_expr(ast, inner);
            ast.expression_await(SPAN, expr)
        }
    }
}
