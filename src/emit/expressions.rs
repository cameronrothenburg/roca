use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::ast::NumberBase;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::helpers::make_error;

pub(crate) fn build_expr<'a>(ast: &AstBuilder<'a>, expr: &roca::Expr) -> Expression<'a> {
    match expr {
        roca::Expr::String(s) => {
            let s = ast.str(s);
            ast.expression_string_literal(SPAN, s, None)
        }
        roca::Expr::Number(n) => {
            ast.expression_numeric_literal(SPAN, *n, None, NumberBase::Decimal)
        }
        roca::Expr::Bool(b) => {
            ast.expression_boolean_literal(SPAN, *b)
        }
        roca::Expr::Ident(name) => {
            if name == "Ok" {
                ast.expression_null_literal(SPAN)
            } else {
                let n = ast.str(name);
                ast.expression_identifier(SPAN, n)
            }
        }
        roca::Expr::SelfRef => {
            ast.expression_this(SPAN)
        }
        roca::Expr::ErrRef(name) => {
            make_error(ast, name)
        }
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
                    ast.expression_binary(SPAN, l, js_op, r)
                }
            }
        }
        roca::Expr::Call { target, args } => {
            let callee = build_expr(ast, target);
            let mut oxc_args = ast.vec();
            for a in args {
                oxc_args.push(Argument::from(build_expr(ast, a)));
            }
            ast.expression_call(SPAN, callee, NONE, oxc_args, false)
        }
        roca::Expr::FieldAccess { target, field } => {
            let obj = build_expr(ast, target);
            let f = ast.str(field);
            Expression::from(ast.member_expression_static(SPAN, obj, ast.identifier_name(SPAN, f), false))
        }
        roca::Expr::StructLit { name, fields } => {
            let mut props = ast.vec();
            for (key, val) in fields {
                let k = ast.str(key);
                let value = build_expr(ast, val);
                let prop = ast.object_property_kind_object_property(
                    SPAN, PropertyKind::Init,
                    PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, k))),
                    value, false, false, false,
                );
                props.push(prop);
            }
            let obj = ast.expression_object(SPAN, props);
            // new Name({ fields })
            let n = ast.str(name);
            let callee = ast.expression_identifier(SPAN, n);
            let mut args = ast.vec();
            args.push(Argument::from(obj));
            ast.expression_new(SPAN, callee, NONE, args)
        }
        roca::Expr::Array(elements) => {
            let mut items = ast.vec();
            for el in elements {
                items.push(ArrayExpressionElement::from(build_expr(ast, el)));
            }
            ast.expression_array(SPAN, items)
        }
        roca::Expr::Index { target, index } => {
            let obj = build_expr(ast, target);
            let idx = build_expr(ast, index);
            Expression::from(ast.member_expression_computed(SPAN, obj, idx, false))
        }
        roca::Expr::Match { value, arms } => {
            // Emit as nested ternaries: val === p1 ? r1 : val === p2 ? r2 : default
            // Build from the end (default first, then wrap)
            let mut result: Option<Expression<'a>> = None;

            for arm in arms.iter().rev() {
                match &arm.pattern {
                    None => {
                        // Default arm
                        result = Some(build_expr(ast, &arm.value));
                    }
                    Some(pattern) => {
                        let val = build_expr(ast, value);
                        let pat = build_expr(ast, pattern);
                        let test = ast.expression_binary(SPAN, val, BinaryOperator::StrictEquality, pat);
                        let consequent = build_expr(ast, &arm.value);
                        let alternate = result.unwrap_or_else(|| ast.expression_identifier(SPAN, "undefined"));
                        result = Some(ast.expression_conditional(SPAN, test, consequent, alternate));
                    }
                }
            }

            result.unwrap_or_else(|| ast.expression_identifier(SPAN, "undefined"))
        }
    }
}
