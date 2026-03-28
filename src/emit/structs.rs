use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

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
        build_class_method(ast, method, &mut class_body);
    }

    // Satisfies methods
    for method in satisfies_methods {
        build_class_method(ast, method, &mut class_body);
    }

    let body = ast.class_body(SPAN, class_body);

    ast.class(SPAN, ClassType::ClassDeclaration, ast.vec(), Some(id), NONE, None, NONE, ast.vec(), body, false, false)
}

fn build_constructor<'a>(
    ast: &AstBuilder<'a>,
    fields: &[roca::Field],
    elements: &mut oxc_allocator::Vec<'a, ClassElement<'a>>,
) {
    let param_name = ast.str("init");
    let pattern = ast.binding_pattern_binding_identifier(SPAN, param_name);
    let mut ctor_params = ast.vec();
    ctor_params.push(ast.plain_formal_parameter(SPAN, pattern));
    let ctor_formal = ast.formal_parameters(SPAN, FormalParameterKind::FormalParameter, ctor_params, NONE);

    let mut stmts = ast.vec();
    for field in fields {
        let f = ast.str(&field.name);
        // this.field = init.field
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

    let body = ast.function_body(SPAN, ast.vec(), stmts);
    let func = ast.function(
        SPAN, FunctionType::FunctionExpression, None, false, false, false,
        NONE, NONE, ctor_formal, NONE, Some(body),
    );
    let ctor_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, "constructor")));
    let ctor = ast.class_element_method_definition(
        SPAN, MethodDefinitionType::MethodDefinition, ast.vec(),
        ctor_key, func, MethodDefinitionKind::Constructor,
        false, false, false, false, None,
    );
    elements.push(ctor);
}

fn build_class_method<'a>(
    ast: &AstBuilder<'a>,
    method: &roca::FnDef,
    elements: &mut oxc_allocator::Vec<'a, ClassElement<'a>>,
) {
    let mut params_list = ast.vec();
    for p in &method.params {
        let pn = ast.str(&p.name);
        let pattern = ast.binding_pattern_binding_identifier(SPAN, pn);
        params_list.push(ast.plain_formal_parameter(SPAN, pattern));
    }
    let formal_params = ast.formal_parameters(SPAN, FormalParameterKind::FormalParameter, params_list, NONE);

    let mut stmts = ast.vec();
    for s in &method.body {
        stmts.push(build_stmt(ast, s, method.returns_err));
    }
    let body = ast.function_body(SPAN, ast.vec(), stmts);

    let uses_self = body_uses_self(&method.body);
    let is_static = !uses_self && !method.params.iter().any(|p| p.name == "self");

    let func = ast.function(
        SPAN, FunctionType::FunctionExpression, None, false, false, false,
        NONE, NONE, formal_params, NONE, Some(body),
    );

    let method_name = ast.str(&method.name);
    let key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, method_name)));
    let method_def = ast.class_element_method_definition(
        SPAN, MethodDefinitionType::MethodDefinition, ast.vec(),
        key, func, MethodDefinitionKind::Method,
        false, is_static, false, false, None,
    );
    elements.push(method_def);
}

/// Check if a function body references `self` anywhere
fn body_uses_self(stmts: &[roca::Stmt]) -> bool {
    stmts.iter().any(|s| stmt_uses_self(s))
}

fn stmt_uses_self(stmt: &roca::Stmt) -> bool {
    match stmt {
        roca::Stmt::Const { value, .. }
        | roca::Stmt::Let { value, .. }
        | roca::Stmt::Assign { value, .. }
        | roca::Stmt::Return(value)
        | roca::Stmt::Expr(value) => expr_uses_self(value),
        roca::Stmt::LetResult { value, .. } => expr_uses_self(value),
        roca::Stmt::ReturnErr(_) => false,
        roca::Stmt::If { condition, then_body, else_body } => {
            expr_uses_self(condition)
                || body_uses_self(then_body)
                || else_body.as_ref().map_or(false, |b| body_uses_self(b))
        }
        roca::Stmt::For { iter, body, .. } => {
            expr_uses_self(iter) || body_uses_self(body)
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
        _ => false,
    }
}
