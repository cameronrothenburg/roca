//! Shape emission traits — each Roca AST shape has a typed conversion to JS.
//! Children are always emitted through the same pipeline, preventing invalid code construction.
//!
//! To add a new expression shape:
//! 1. Add the AST variant in src/ast/expr.rs
//! 2. Add a `js_*` function here
//! 3. Wire it into `expr_to_js`
//! That's it. Children go through `expr_to_js` — the compiler enforces the pipeline.

use roca_ast::{self as roca, Expr, BinOp, MatchPattern, StringPart};
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::ast_helpers::*;
use super::helpers::{make_error_simple, make_result, null};

use std::collections::HashSet;
use std::cell::RefCell;

thread_local! {
    static STDLIB_CONTRACTS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Register stdlib contract names for `roca.` prefixing in JS output.
pub(crate) fn set_stdlib_contracts(names: HashSet<String>) {
    STDLIB_CONTRACTS.with(|c| *c.borrow_mut() = names);
}

fn is_stdlib_contract(name: &str) -> bool {
    STDLIB_CONTRACTS.with(|c| c.borrow().contains(name))
}

// ─── Expression shapes ───────────────────────────────────
// Each function takes the AST shape's fields and returns an OXC Expression.
// Children are emitted by calling `expr_to_js` recursively.

pub(crate) fn expr_to_js<'a>(ast: &AstBuilder<'a>, expr: &roca::Expr) -> Expression<'a> {
    match expr {
        Expr::Number(n) => js_number(ast, *n),
        Expr::String(s) => js_string(ast, s),
        Expr::Bool(v) => js_bool(ast, *v),
        Expr::Null => js_null(ast),
        Expr::SelfRef => js_self(ast),
        Expr::Ident(name) => js_ident(ast, name),
        Expr::Not(inner) => js_not(ast, inner),
        Expr::BinOp { left, op, right } => js_binop(ast, left, op, right),
        Expr::Call { target, args } => js_call(ast, target, args),
        Expr::FieldAccess { target, field } => js_field_access(ast, target, field),
        Expr::StructLit { name, fields } => js_struct_lit(ast, name, fields),
        Expr::Array(elements) => js_array(ast, elements),
        Expr::Index { target, index } => js_index(ast, target, index),
        Expr::StringInterp(parts) => js_string_interp(ast, parts),
        Expr::Match { value, arms } => js_match(ast, value, arms),
        Expr::Closure { params, body } => js_closure(ast, params, body),
        Expr::Await(inner) => js_await(ast, inner),
        Expr::EnumVariant { enum_name, variant, args } => js_enum_variant(ast, enum_name, variant, args),
    }
}

// ─── Leaf shapes (no children) ────────────────────────────

fn js_number<'a>(ast: &AstBuilder<'a>, n: f64) -> Expression<'a> {
    number_lit(ast, n)
}

fn js_string<'a>(ast: &AstBuilder<'a>, s: &str) -> Expression<'a> {
    string_lit(ast, s)
}

fn js_bool<'a>(ast: &AstBuilder<'a>, v: bool) -> Expression<'a> {
    bool_lit(ast, v)
}

fn js_null<'a>(ast: &AstBuilder<'a>) -> Expression<'a> {
    null_lit(ast)
}

fn js_self<'a>(ast: &AstBuilder<'a>) -> Expression<'a> {
    ast.expression_this(SPAN)
}

fn js_ident<'a>(ast: &AstBuilder<'a>, name: &str) -> Expression<'a> {
    if name == "Ok" { return null_lit(ast); }
    // Stdlib contracts accessed via roca.ContractName
    if is_stdlib_contract(name) {
        let roca_obj = ident(ast, "roca");
        let n = ast.str(name);
        return Expression::from(ast.member_expression_static(SPAN, roca_obj, ast.identifier_name(SPAN, n), false));
    }
    ident(ast, name)
}

// ─── Unary shapes ─────────────────────────────────────────

fn js_not<'a>(ast: &AstBuilder<'a>, inner: &Expr) -> Expression<'a> {
    unary_not(ast, expr_to_js(ast, inner))
}

fn js_await<'a>(ast: &AstBuilder<'a>, inner: &Expr) -> Expression<'a> {
    ast.expression_await(SPAN, expr_to_js(ast, inner))
}

// ─── Binary shapes ────────────────────────────────────────

fn js_binop<'a>(ast: &AstBuilder<'a>, left: &Expr, op: &BinOp, right: &Expr) -> Expression<'a> {
    let l = expr_to_js(ast, left);
    let r = expr_to_js(ast, right);
    match op {
        BinOp::And => ast.expression_logical(SPAN, l, LogicalOperator::And, r),
        BinOp::Or => ast.expression_logical(SPAN, l, LogicalOperator::Or, r),
        _ => {
            let js_op = match op {
                BinOp::Add => BinaryOperator::Addition,
                BinOp::Sub => BinaryOperator::Subtraction,
                BinOp::Mul => BinaryOperator::Multiplication,
                BinOp::Div => BinaryOperator::Division,
                BinOp::Eq => BinaryOperator::StrictEquality,
                BinOp::Neq => BinaryOperator::StrictInequality,
                BinOp::Lt => BinaryOperator::LessThan,
                BinOp::Gt => BinaryOperator::GreaterThan,
                BinOp::Lte => BinaryOperator::LessEqualThan,
                BinOp::Gte => BinaryOperator::GreaterEqualThan,
                BinOp::And | BinOp::Or => unreachable!(),
            };
            binary(ast, l, js_op, r)
        }
    }
}

// ─── Compound shapes ──────────────────────────────────────

fn js_call<'a>(ast: &AstBuilder<'a>, target: &Expr, args: &[Expr]) -> Expression<'a> {
    // Console methods
    if let Expr::Ident(name) = target {
        let console_method = match name.as_str() {
            "log" => Some("log"),
            "error" => Some("error"),
            "warn" => Some("warn"),
            _ => None,
        };
        if let Some(method) = console_method {
            let callee = static_field(ast, "console", method);
            let oxc_args = build_args(ast, args);
            return ast.expression_call(SPAN, callee, NONE, oxc_args, false);
        }
    }

    // Uppercase = type conversion or constructor
    if let Expr::Ident(name) = target {
        if name.chars().next().map_or(false, |c| c.is_uppercase()) {
            let oxc_args = build_args(ast, args);
            match name.as_str() {
                "String" | "Number" => return ast.expression_call(SPAN, ident(ast, name), NONE, oxc_args, false),
                "Bool" => return ast.expression_call(SPAN, ident(ast, "Boolean"), NONE, oxc_args, false),
                _ => return ast.expression_new(SPAN, ident(ast, name), NONE, oxc_args),
            }
        }
    }

    let callee = expr_to_js(ast, target);
    let oxc_args = build_args(ast, args);
    ast.expression_call(SPAN, callee, NONE, oxc_args, false)
}

fn js_field_access<'a>(ast: &AstBuilder<'a>, target: &Expr, field_name: &str) -> Expression<'a> {
    let obj = expr_to_js(ast, target);
    let f = ast.str(field_name);
    Expression::from(ast.member_expression_static(SPAN, obj, ast.identifier_name(SPAN, f), false))
}

fn js_struct_lit<'a>(ast: &AstBuilder<'a>, name: &str, fields: &[(String, Expr)]) -> Expression<'a> {
    let mut props_list = ast.vec();
    for (key, val) in fields {
        props_list.push(prop(ast, key, expr_to_js(ast, val)));
    }
    let obj = object_expr(ast, props_list);
    let mut call_args = ast.vec();
    call_args.push(arg(obj));
    ast.expression_new(SPAN, ident(ast, name), NONE, call_args)
}

fn js_array<'a>(ast: &AstBuilder<'a>, elements: &[Expr]) -> Expression<'a> {
    let mut items = ast.vec();
    for el in elements {
        items.push(ArrayExpressionElement::from(expr_to_js(ast, el)));
    }
    ast.expression_array(SPAN, items)
}

fn js_index<'a>(ast: &AstBuilder<'a>, target: &Expr, index: &Expr) -> Expression<'a> {
    let obj = expr_to_js(ast, target);
    let idx = expr_to_js(ast, index);
    Expression::from(ast.member_expression_computed(SPAN, obj, idx, false))
}

fn js_closure<'a>(ast: &AstBuilder<'a>, params: &[String], body: &Expr) -> Expression<'a> {
    let mut param_list = ast.vec();
    for p in params {
        param_list.push(param(ast, p));
    }
    let fp = ast.formal_parameters(SPAN, FormalParameterKind::ArrowFormalParameters, param_list, NONE);
    let body_expr = expr_to_js(ast, body);
    let body_stmt = expr_stmt(ast, body_expr);
    let mut stmts = ast.vec();
    stmts.push(body_stmt);
    let fn_body = function_body(ast, stmts);
    ast.expression_arrow_function(SPAN, true, false, NONE, fp, NONE, fn_body)
}

fn js_enum_variant<'a>(ast: &AstBuilder<'a>, enum_name: &str, variant: &str, args: &[Expr]) -> Expression<'a> {
    let target = field_access(ast, ident(ast, enum_name), ast.str(variant));
    if args.is_empty() {
        target
    } else {
        let call_args = build_args(ast, args);
        ast.expression_call(SPAN, target, NONE, call_args, false)
    }
}

// ─── String interpolation ─────────────────────────────────

fn js_string_interp<'a>(ast: &AstBuilder<'a>, parts: &[StringPart]) -> Expression<'a> {
    let mut quasis = ast.vec();
    let mut expressions = ast.vec();

    for (i, part) in parts.iter().enumerate() {
        match part {
            StringPart::Literal(s) => {
                let raw: oxc_span::Atom = ast.str(s).into();
                let tail = i == parts.len() - 1;
                let value = TemplateElementValue { raw, cooked: None };
                quasis.push(ast.template_element(SPAN, value, tail, false));
            }
            StringPart::Expr(expr) => {
                if quasis.is_empty() {
                    let raw: oxc_span::Atom = ast.str("").into();
                    let value = TemplateElementValue { raw, cooked: None };
                    quasis.push(ast.template_element(SPAN, value, false, false));
                }
                expressions.push(expr_to_js(ast, expr));
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

// ─── Match ────────────────────────────────────────────────

pub(crate) fn match_has_err_arms(arms: &[roca::MatchArm]) -> bool {
    arms.iter().any(|arm| {
        if let Expr::FieldAccess { target, .. } = &arm.value {
            if let Expr::Ident(name) = target.as_ref() { return name == "err"; }
        }
        false
    })
}

fn build_match_arm_value<'a>(ast: &AstBuilder<'a>, expr: &Expr, mixed: bool) -> Expression<'a> {
    if let Expr::FieldAccess { target, field } = expr {
        if let Expr::Ident(name) = target.as_ref() {
            if name == "err" {
                return make_result(ast, null(ast), make_error_simple(ast, field));
            }
        }
    }
    let val = expr_to_js(ast, expr);
    if mixed { make_result(ast, val, null(ast)) } else { val }
}

fn js_match<'a>(ast: &AstBuilder<'a>, value: &Expr, arms: &[roca::MatchArm]) -> Expression<'a> {
    // If value is a simple ident, use it directly. Otherwise wrap in IIFE
    // to evaluate once: ((_m) => ternary_chain)(<value>)
    let is_simple = matches!(value, Expr::Ident(_));
    if is_simple {
        js_match_inner(ast, value, arms)
    } else {
        let temp = "_m";
        let temp_expr = Expr::Ident(temp.to_string());
        let ternary = js_match_inner(ast, &temp_expr, arms);
        let mut fn_params = ast.vec();
        fn_params.push(param(ast, temp));
        let params = formal_params(ast, fn_params);
        let mut stmts = ast.vec();
        stmts.push(return_stmt(ast, ternary));
        let body = function_body(ast, stmts);
        let arrow = ast.expression_arrow_function(SPAN, false, false, NONE, params, NONE, body);
        let mut call_args = ast.vec();
        call_args.push(arg(expr_to_js(ast, value)));
        ast.expression_call(SPAN, arrow, NONE, call_args, false)
    }
}

/// Above this arm count, emit an IIFE with if-else statements instead of nested ternaries.
const MATCH_IIFE_THRESHOLD: usize = 8;

fn js_match_inner<'a>(ast: &AstBuilder<'a>, value: &Expr, arms: &[roca::MatchArm]) -> Expression<'a> {
    if arms.len() > MATCH_IIFE_THRESHOLD {
        return js_match_as_iife(ast, value, arms);
    }

    let mixed = match_has_err_arms(arms);
    let mut result: Option<Expression<'a>> = None;

    for arm in arms.iter().rev() {
        let arm_value = build_match_arm_value(ast, &arm.value, mixed);
        match &arm.pattern {
            None => {
                result = Some(arm_value);
            }
            Some(MatchPattern::Value(pattern)) => {
                let val = expr_to_js(ast, value);
                let pat = expr_to_js(ast, pattern);
                let test = binary(ast, val, BinaryOperator::StrictEquality, pat);
                let alternate = result.unwrap_or_else(|| ident(ast, "undefined"));
                result = Some(ast.expression_conditional(SPAN, test, arm_value, alternate));
            }
            Some(MatchPattern::Variant { enum_name, variant, bindings }) => {
                let val = expr_to_js(ast, value);
                let alternate = result.unwrap_or_else(|| ident(ast, "undefined"));

                // For simple enum variants (no bindings), compare value === EnumName.variant.
                // This works for string/number enums (primitive equality) and unit enums (singleton reference).
                // For data enum variants (with bindings), compare value._tag === "variant" to match and destructure.
                let test = if bindings.is_empty() {
                    let enum_variant = field_access(ast, ident(ast, enum_name), ast.str(variant));
                    binary(ast, val, BinaryOperator::StrictEquality, enum_variant)
                } else {
                    let tag = field_access(ast, val, ast.str(TAG_FIELD));
                    let tag_str = string_lit(ast, variant);
                    binary(ast, tag, BinaryOperator::StrictEquality, tag_str)
                };

                let consequent = if bindings.is_empty() {
                    arm_value
                } else {
                    let mut fn_params = ast.vec();
                    let mut call_args = ast.vec();
                    for (i, binding) in bindings.iter().enumerate() {
                        fn_params.push(param(ast, binding));
                        let field_name = positional_field(i);
                        let val2 = expr_to_js(ast, value);
                        call_args.push(Argument::from(
                            field_access(ast, val2, ast.str(&field_name))
                        ));
                    }
                    let params = formal_params(ast, fn_params);
                    let mut stmts = ast.vec();
                    stmts.push(return_stmt(ast, arm_value));
                    let body = function_body(ast, stmts);
                    let arrow = ast.expression_arrow_function(SPAN, false, false, NONE, params, NONE, body);
                    ast.expression_call(SPAN, arrow, NONE, call_args, false)
                };

                result = Some(ast.expression_conditional(SPAN, test, consequent, alternate));
            }
        }
    }

    result.unwrap_or_else(|| ident(ast, "undefined"))
}

/// Emit a match as an IIFE containing a flat if-else chain. Used when arm count exceeds
/// `MATCH_IIFE_THRESHOLD` to avoid deeply nested ternaries that are hard to read and
/// can hit JS engine recursion limits.
///
/// Emits: `(() => { if (v === p1) return arm1; if (v === p2) return arm2; ... return undefined; })()`
fn js_match_as_iife<'a>(ast: &AstBuilder<'a>, value: &Expr, arms: &[roca::MatchArm]) -> Expression<'a> {
    let mixed = match_has_err_arms(arms);
    let mut stmts = ast.vec();

    for arm in arms.iter() {
        let arm_value = build_match_arm_value(ast, &arm.value, mixed);
        match &arm.pattern {
            None => {
                stmts.push(return_stmt(ast, arm_value));
                break; // wildcard is the default; nothing after it is reachable
            }
            Some(MatchPattern::Value(pattern)) => {
                let val = expr_to_js(ast, value);
                let pat = expr_to_js(ast, pattern);
                let test = binary(ast, val, BinaryOperator::StrictEquality, pat);
                let mut then_stmts = ast.vec();
                then_stmts.push(return_stmt(ast, arm_value));
                stmts.push(if_stmt(ast, test, block(ast, then_stmts), None));
            }
            Some(MatchPattern::Variant { enum_name, variant, bindings }) => {
                let val = expr_to_js(ast, value);
                let test = if bindings.is_empty() {
                    let enum_variant = field_access(ast, ident(ast, enum_name), ast.str(variant));
                    binary(ast, val, BinaryOperator::StrictEquality, enum_variant)
                } else {
                    let tag = field_access(ast, val, ast.str(TAG_FIELD));
                    let tag_str = string_lit(ast, variant);
                    binary(ast, tag, BinaryOperator::StrictEquality, tag_str)
                };

                let consequent_expr = if bindings.is_empty() {
                    arm_value
                } else {
                    let mut fn_params = ast.vec();
                    let mut call_args = ast.vec();
                    for (i, binding) in bindings.iter().enumerate() {
                        fn_params.push(param(ast, binding));
                        let field_name = positional_field(i);
                        let val2 = expr_to_js(ast, value);
                        call_args.push(Argument::from(
                            field_access(ast, val2, ast.str(&field_name))
                        ));
                    }
                    let params = formal_params(ast, fn_params);
                    let mut inner_stmts = ast.vec();
                    inner_stmts.push(return_stmt(ast, arm_value));
                    let body = function_body(ast, inner_stmts);
                    let arrow = ast.expression_arrow_function(SPAN, false, false, NONE, params, NONE, body);
                    ast.expression_call(SPAN, arrow, NONE, call_args, false)
                };

                let mut then_stmts = ast.vec();
                then_stmts.push(return_stmt(ast, consequent_expr));
                stmts.push(if_stmt(ast, test, block(ast, then_stmts), None));
            }
        }
    }

    stmts.push(return_stmt(ast, ident(ast, "undefined")));

    let body = function_body(ast, stmts);
    let params = formal_params(ast, ast.vec());
    let arrow = ast.expression_arrow_function(SPAN, false, false, NONE, params, NONE, body);
    ast.expression_call(SPAN, arrow, NONE, ast.vec(), false)
}

// ─── Shared helpers ───────────────────────────────────────

fn build_args<'a>(ast: &AstBuilder<'a>, args: &[Expr]) -> oxc_allocator::Vec<'a, Argument<'a>> {
    let mut oxc_args = ast.vec();
    for a in args {
        oxc_args.push(arg(expr_to_js(ast, a)));
    }
    oxc_args
}
