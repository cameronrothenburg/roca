//! Struct codegen — emits Roca structs as JS classes.
//! Generates constructors, methods, and `satisfies` trait implementations.

use roca_ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::ast_helpers::{param, formal_params, function_body, prop_key};
use super::statements::build_stmt;

/// Build a struct as a class declaration, including satisfies methods
pub(crate) fn build_struct<'a>(
    ast: &AstBuilder<'a>,
    s: &roca::StructDef,
    satisfies_methods: &[&roca::FnDef],
) -> Class<'a> {
    let n = ast.str(&s.name);
    let id = ast.binding_identifier(SPAN, n);

    let mut class_body = ast.vec();

    // Constructor (if has fields)
    if !s.fields.is_empty() {
        build_constructor(ast, &s.fields, &mut class_body);
    }

    // Struct's own methods
    for method in &s.methods {
        let sig_errors = s.signatures.iter()
            .find(|sig| sig.name == method.name)
            .map(|sig| sig.errors.as_slice())
            .unwrap_or(&[]);
        build_class_method(ast, method, sig_errors, false, &mut class_body);
    }

    // Satisfies methods — always instance
    for method in satisfies_methods {
        build_class_method(ast, method, &[], true, &mut class_body);
    }

    let body = ast.class_body(SPAN, class_body);

    ast.class(SPAN, ClassType::ClassDeclaration, ast.vec(), Some(id), NONE, None, NONE, ast.vec(), body, false, false)
}

fn build_constructor<'a>(
    ast: &AstBuilder<'a>,
    fields: &[roca::Field],
    elements: &mut oxc_allocator::Vec<'a, ClassElement<'a>>,
) {
    let mut ctor_params = ast.vec();
    ctor_params.push(param(ast, "init"));
    let ctor_formal = formal_params(ast, ctor_params);

    let mut stmts = ast.vec();

    // Emit constraint validation guards before assignments
    for field in fields {
        emit_field_guards(ast, field, &mut stmts);
    }

    for field in fields {
        let f = ast.str(&field.name);
        let this_member = ast.member_expression_static(
            SPAN, ast.expression_this(SPAN), ast.identifier_name(SPAN, f), false,
        );
        let target = AssignmentTarget::from(SimpleAssignmentTarget::from(this_member));

        let f2 = ast.str(&field.name);
        let init_member = Expression::from(ast.member_expression_static(
            SPAN, ast.expression_identifier(SPAN, "init"), ast.identifier_name(SPAN, f2), false,
        ));

        let assign = ast.expression_assignment(SPAN, AssignmentOperator::Assign, target, init_member);
        stmts.push(ast.statement_expression(SPAN, assign));
    }

    let body = function_body(ast, stmts);
    let func = ast.function(
        SPAN, FunctionType::FunctionExpression, None, false, false, false,
        NONE, NONE, ctor_formal, NONE, Some(body),
    );
    let ctor_key = prop_key(ast, "constructor");
    let ctor = ast.class_element_method_definition(
        SPAN, MethodDefinitionType::MethodDefinition, ast.vec(),
        ctor_key, func, MethodDefinitionKind::Constructor,
        false, false, false, false, None,
    );
    elements.push(ctor);
}

fn emit_field_guards<'a>(
    ast: &AstBuilder<'a>,
    field: &roca::Field,
    stmts: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) {
    if field.constraints.is_empty() { return; }
    let is_string = matches!(field.type_ref, roca::TypeRef::String);
    let fname = field.name.clone();
    super::functions::emit_constraint_guards(
        ast, &field.name, is_string, &field.constraints,
        &move |a| init_field(a, &fname), stmts,
    );
}

fn init_field<'a>(ast: &AstBuilder<'a>, name: &str) -> Expression<'a> {
    let f = ast.str(name);
    Expression::from(ast.member_expression_static(
        SPAN, ast.expression_identifier(SPAN, "init"), ast.identifier_name(SPAN, f), false,
    ))
}

fn build_class_method<'a>(
    ast: &AstBuilder<'a>,
    method: &roca::FnDef,
    sig_errors: &[roca::ErrDecl],
    force_instance: bool,
    elements: &mut oxc_allocator::Vec<'a, ClassElement<'a>>,
) {
    let mut params_list = ast.vec();
    for p in &method.params {
        params_list.push(param(ast, &p.name));
    }
    let method_params = formal_params(ast, params_list);

    let mut stmts = ast.vec();
    for s in &method.body {
        for emitted in build_stmt(ast, s, method.returns_err, &method.return_type, sig_errors, method.crash.as_ref()) {
            stmts.push(emitted);
        }
    }
    let body = function_body(ast, stmts);

    let uses_self = body_uses_self(&method.body);
    let is_static = !force_instance && !uses_self && !method.params.iter().any(|p| p.name == "self");
    let is_async = super::functions::body_has_wait(&method.body);

    let func = ast.function(
        SPAN, FunctionType::FunctionExpression, None, false, is_async, false,
        NONE, NONE, method_params, NONE, Some(body),
    );

    let key = prop_key(ast, &method.name);
    let method_def = ast.class_element_method_definition(
        SPAN, MethodDefinitionType::MethodDefinition, ast.vec(),
        key, func, MethodDefinitionKind::Method,
        false, is_static, false, false, None,
    );
    elements.push(method_def);
}

/// Check if a function body references `self` anywhere
pub(crate) fn body_uses_self(stmts: &[roca::Stmt]) -> bool {
    stmts.iter().any(|s| stmt_uses_self(s))
}

fn stmt_uses_self(stmt: &roca::Stmt) -> bool {
    match stmt {
        roca::Stmt::Const { value, .. }
        | roca::Stmt::Let { value, .. }
        | roca::Stmt::Assign { value, .. }
        | roca::Stmt::FieldAssign { value, .. }
        | roca::Stmt::Return(value)
        | roca::Stmt::Expr(value) => expr_uses_self(value),
        roca::Stmt::LetResult { value, .. } => expr_uses_self(value),
        roca::Stmt::ReturnErr { .. } => false,
        roca::Stmt::If { condition, then_body, else_body } => {
            expr_uses_self(condition)
                || body_uses_self(then_body)
                || else_body.as_ref().map_or(false, |b| body_uses_self(b))
        }
        roca::Stmt::For { iter, body, .. } => {
            expr_uses_self(iter) || body_uses_self(body)
        }
        roca::Stmt::While { condition, body } => {
            expr_uses_self(condition) || body_uses_self(body)
        }
        roca::Stmt::Break | roca::Stmt::Continue => false,
        roca::Stmt::Wait { kind, .. } => {
            match kind {
                roca::WaitKind::Single(e) => expr_uses_self(e),
                roca::WaitKind::All(es) | roca::WaitKind::First(es) => es.iter().any(|e| expr_uses_self(e)),
            }
        }
    }
}

fn expr_uses_self(expr: &roca::Expr) -> bool {
    match expr {
        roca::Expr::SelfRef => true,
        roca::Expr::BinOp { left, right, .. } => expr_uses_self(left) || expr_uses_self(right),
        roca::Expr::Call { target, args } => {
            expr_uses_self(target) || args.iter().any(|a| expr_uses_self(a))
        }
        roca::Expr::FieldAccess { target, .. } => expr_uses_self(target),
        roca::Expr::StructLit { fields, .. } => fields.iter().any(|(_, v)| expr_uses_self(v)),
        roca::Expr::Array(elements) => elements.iter().any(|e| expr_uses_self(e)),
        roca::Expr::Index { target, index } => expr_uses_self(target) || expr_uses_self(index),
        roca::Expr::Match { value, arms } => {
            expr_uses_self(value) || arms.iter().any(|a| {
                a.pattern.as_ref().map_or(false, |p| match p {
                    roca::MatchPattern::Value(e) => expr_uses_self(e),
                    roca::MatchPattern::Variant { .. } => false,
                }) || expr_uses_self(&a.value)
            })
        }
        roca::Expr::EnumVariant { args, .. } => args.iter().any(|a| expr_uses_self(a)),
        _ => false,
    }
}
