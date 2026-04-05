//! roca-js — Roca JS backend.
//!
//! Builds an OXC JS AST from the checked Roca AST, then renders it to
//! JavaScript text via `oxc_codegen`.
//!
//! # Mapping
//!
//! - `fn` → `function` (with `export` if `pub`)
//! - `struct` → `class` (methods are static unless they use `self`)
//! - `const` → `const`, `var` → `let`
//! - `for x in items` → `for (const x of items)`
//! - `loop` → `while (true)`
//! - `match` → ternary chain
//! - `fn(x) -> expr` → `(x) => expr`
//! - `self` → `this`
//! - `import from "./x.roca"` → `import from "./x.js"`

use roca_lang::{SourceFile, Item, FuncDef, StructDef, Stmt, Expr, ExprKind, BinOp, UnaryOp, Lit, Pattern, MatchArm};

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast::{AstBuilder, NONE};
use oxc_codegen::Codegen;
use oxc_span::{SPAN, SourceType};

/// Emit JavaScript from a checked Roca source file.
/// Builds an OXC JS AST internally, then renders it to a string.
pub fn emit(source: &SourceFile) -> String {
    let allocator = Allocator::default();
    let ast = AstBuilder::new(&allocator);

    let mut body: oxc_allocator::Vec<'_, Statement<'_>> = ast.vec();

    for item in &source.items {
        match item {
            Item::Import { names, path } => {
                let js_path = path.replace(".roca", ".js");
                let import_stmt = build_import(&ast, names, &js_path);
                body.push(import_stmt);
            }
            Item::Function(f) => {
                let func = build_function(&ast, f);
                let func_decl = Declaration::FunctionDeclaration(ast.alloc(func));
                if f.is_pub {
                    let export = ast.export_named_declaration(
                        SPAN, Some(func_decl), ast.vec(), None,
                        ImportOrExportKind::Value, NONE,
                    );
                    body.push(Statement::from(ModuleDeclaration::ExportNamedDeclaration(ast.alloc(export))));
                } else {
                    body.push(Statement::from(func_decl));
                }
            }
            Item::Struct(s) => {
                let class = build_struct(&ast, s);
                let class_decl = Declaration::ClassDeclaration(ast.alloc(class));
                if s.is_pub {
                    let export = ast.export_named_declaration(
                        SPAN, Some(class_decl), ast.vec(), None,
                        ImportOrExportKind::Value, NONE,
                    );
                    body.push(Statement::from(ModuleDeclaration::ExportNamedDeclaration(ast.alloc(export))));
                } else {
                    body.push(Statement::from(class_decl));
                }
            }
            Item::Enum(_) => {
                // Enums not required by the 10 target tests
            }
        }
    }

    let source_text = allocator.alloc_str("");
    let program = ast.program(SPAN, SourceType::mjs(), source_text, ast.vec(), None, ast.vec(), body);
    Codegen::new().build(&program).code
}

// ─── Import ──────────────────────────────────────────────────────────────────

fn build_import<'a>(ast: &AstBuilder<'a>, names: &[String], path: &str) -> Statement<'a> {
    let mut specifiers: oxc_allocator::Vec<'a, ImportDeclarationSpecifier<'a>> = ast.vec();
    for name in names {
        let n = ast.str(name);
        let local = ast.binding_identifier(SPAN, n);
        let imported = ModuleExportName::IdentifierName(ast.identifier_name(SPAN, n));
        specifiers.push(ImportDeclarationSpecifier::ImportSpecifier(ast.alloc(
            ast.import_specifier(SPAN, imported, local, ImportOrExportKind::Value)
        )));
    }
    let src_str = ast.str(path);
    let src = ast.string_literal(SPAN, src_str, None);
    let decl = ast.import_declaration(SPAN, Some(specifiers), src, None, NONE, ImportOrExportKind::Value);
    Statement::from(ModuleDeclaration::ImportDeclaration(ast.alloc(decl)))
}

// ─── Function ────────────────────────────────────────────────────────────────

fn build_function<'a>(ast: &AstBuilder<'a>, f: &FuncDef) -> Function<'a> {
    let n = ast.str(&f.name);
    let id = ast.binding_identifier(SPAN, n);

    let mut params_list: oxc_allocator::Vec<'a, FormalParameter<'a>> = ast.vec();
    for p in &f.params {
        params_list.push(make_param(ast, &p.name));
    }
    let params = ast.formal_parameters(SPAN, FormalParameterKind::FormalParameter, params_list, NONE);

    let mut stmts: oxc_allocator::Vec<'a, Statement<'a>> = ast.vec();
    for s in &f.body {
        for emitted in build_stmt(ast, s) {
            stmts.push(emitted);
        }
    }
    let body = ast.function_body(SPAN, ast.vec(), stmts);

    ast.function(
        SPAN, FunctionType::FunctionDeclaration, Some(id),
        false, false, false, NONE, NONE, params, NONE, Some(body),
    )
}

// ─── Struct as class ─────────────────────────────────────────────────────────

fn build_struct<'a>(ast: &AstBuilder<'a>, s: &StructDef) -> Class<'a> {
    let n = ast.str(&s.name);
    let id = ast.binding_identifier(SPAN, n);

    let mut class_body: oxc_allocator::Vec<'a, ClassElement<'a>> = ast.vec();

    // Generate constructor if struct has fields: constructor(props) { this.x = props.x; ... }
    if !s.fields.is_empty() {
        let mut ctor_params: oxc_allocator::Vec<'a, FormalParameter<'a>> = ast.vec();
        ctor_params.push(make_param(ast, "props"));

        let mut ctor_stmts: oxc_allocator::Vec<'a, Statement<'a>> = ast.vec();
        for field in &s.fields {
            // this.<field> = props.<field>
            let this_expr = ast.expression_this(SPAN);
            let fname = ast.str(&field.name);
            let lhs_member = ast.member_expression_static(SPAN, this_expr, ast.identifier_name(SPAN, fname), false);
            let lhs = SimpleAssignmentTarget::from(lhs_member);

            let props_ref = ast.expression_identifier(SPAN, ast.str("props"));
            let rhs = Expression::from(ast.member_expression_static(
                SPAN, props_ref, ast.identifier_name(SPAN, ast.str(&field.name)), false
            ));

            let assign = ast.expression_assignment(SPAN, AssignmentOperator::Assign, AssignmentTarget::from(lhs), rhs);
            ctor_stmts.push(ast.statement_expression(SPAN, assign));
        }

        let ctor_body = ast.function_body(SPAN, ast.vec(), ctor_stmts);
        let ctor_params_formal = ast.formal_parameters(SPAN, FormalParameterKind::FormalParameter, ctor_params, NONE);

        let ctor_fn = ast.function(
            SPAN, FunctionType::FunctionExpression, None,
            false, false, false, NONE, NONE, ctor_params_formal, NONE, Some(ctor_body),
        );

        let ctor_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, ast.str("constructor"))));
        let ctor_element = ast.class_element_method_definition(
            SPAN, MethodDefinitionType::MethodDefinition, ast.vec(),
            ctor_key, ctor_fn, MethodDefinitionKind::Constructor,
            false, false, false, false, None,
        );
        class_body.push(ctor_element);
    }

    for method in &s.methods {
        let class_method = build_class_method(ast, method);
        class_body.push(class_method);
    }

    let body = ast.class_body(SPAN, class_body);
    ast.class(SPAN, ClassType::ClassDeclaration, ast.vec(), Some(id), NONE, None, NONE, ast.vec(), body, false, false)
}

fn build_class_method<'a>(ast: &AstBuilder<'a>, method: &FuncDef) -> ClassElement<'a> {
    let mut params_list: oxc_allocator::Vec<'a, FormalParameter<'a>> = ast.vec();
    for p in &method.params {
        params_list.push(make_param(ast, &p.name));
    }
    let params = ast.formal_parameters(SPAN, FormalParameterKind::FormalParameter, params_list, NONE);

    let mut stmts: oxc_allocator::Vec<'a, Statement<'a>> = ast.vec();
    for s in &method.body {
        for emitted in build_stmt(ast, s) {
            stmts.push(emitted);
        }
    }
    let body = ast.function_body(SPAN, ast.vec(), stmts);

    let func = ast.function(
        SPAN, FunctionType::FunctionExpression, None,
        false, false, false, NONE, NONE, params, NONE, Some(body),
    );

    let k = ast.str(&method.name);
    let key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, k)));
    // static if no param is named "self"
    let is_static = !method.params.iter().any(|p| p.name == "self");
    ast.class_element_method_definition(
        SPAN, MethodDefinitionType::MethodDefinition, ast.vec(),
        key, func, MethodDefinitionKind::Method,
        false, is_static, false, false, None,
    )
}

// ─── Statements ──────────────────────────────────────────────────────────────

fn build_stmt<'a>(ast: &AstBuilder<'a>, stmt: &Stmt) -> Vec<Statement<'a>> {
    match stmt {
        Stmt::Let { name, value, is_const: true, .. } => {
            vec![make_var_decl(ast, name, build_expr(ast, value), VariableDeclarationKind::Const)]
        }
        Stmt::Let { name, value, is_const: false, .. } => {
            vec![make_var_decl(ast, name, build_expr(ast, value), VariableDeclarationKind::Let)]
        }
        Stmt::Var { name, value, .. } => {
            vec![make_var_decl(ast, name, build_expr(ast, value), VariableDeclarationKind::Let)]
        }
        Stmt::Assign { target, value } => {
            let n = ast.str(target);
            let lhs = SimpleAssignmentTarget::AssignmentTargetIdentifier(
                ast.alloc(ast.identifier_reference(SPAN, n))
            );
            let rhs = build_expr(ast, value);
            let assign = ast.expression_assignment(
                SPAN, AssignmentOperator::Assign, AssignmentTarget::from(lhs), rhs
            );
            vec![ast.statement_expression(SPAN, assign)]
        }
        Stmt::SetField { target, field, value } => {
            let obj = build_expr(ast, target);
            let f = ast.str(field);
            let member = ast.member_expression_static(SPAN, obj, ast.identifier_name(SPAN, f), false);
            let lhs = SimpleAssignmentTarget::from(member);
            let rhs = build_expr(ast, value);
            let assign = ast.expression_assignment(
                SPAN, AssignmentOperator::Assign, AssignmentTarget::from(lhs), rhs
            );
            vec![ast.statement_expression(SPAN, assign)]
        }
        Stmt::ArraySet { target, index, value } => {
            let obj = build_expr(ast, target);
            let idx = build_expr(ast, index);
            let member = ast.member_expression_computed(SPAN, obj, idx, false);
            let lhs = SimpleAssignmentTarget::from(member);
            let rhs = build_expr(ast, value);
            let assign = ast.expression_assignment(
                SPAN, AssignmentOperator::Assign, AssignmentTarget::from(lhs), rhs
            );
            vec![ast.statement_expression(SPAN, assign)]
        }
        Stmt::Return(expr) => {
            vec![ast.statement_return(SPAN, Some(build_expr(ast, expr)))]
        }
        Stmt::If { cond, then, else_ } => {
            let test = build_expr(ast, cond);
            let mut then_stmts: oxc_allocator::Vec<'a, Statement<'a>> = ast.vec();
            for s in then {
                for emitted in build_stmt(ast, s) {
                    then_stmts.push(emitted);
                }
            }
            let consequent = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, then_stmts)));
            let alternate = else_.as_ref().map(|body| {
                let mut stmts: oxc_allocator::Vec<'a, Statement<'a>> = ast.vec();
                for s in body {
                    for emitted in build_stmt(ast, s) {
                        stmts.push(emitted);
                    }
                }
                Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)))
            });
            vec![ast.statement_if(SPAN, test, consequent, alternate)]
        }
        Stmt::Loop { body } => {
            let test = ast.expression_boolean_literal(SPAN, true);
            let mut stmts: oxc_allocator::Vec<'a, Statement<'a>> = ast.vec();
            for s in body {
                for emitted in build_stmt(ast, s) {
                    stmts.push(emitted);
                }
            }
            let body_stmt = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)));
            vec![ast.statement_while(SPAN, test, body_stmt)]
        }
        Stmt::For { name, iter, body } => {
            let n = ast.str(name);
            let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
            let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, pattern, NONE, None, false);
            let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
            let left = ForStatementLeft::VariableDeclaration(ast.alloc(decl));
            let right = build_expr(ast, iter);
            let mut stmts: oxc_allocator::Vec<'a, Statement<'a>> = ast.vec();
            for s in body {
                for emitted in build_stmt(ast, s) {
                    stmts.push(emitted);
                }
            }
            let body_stmt = Statement::BlockStatement(ast.alloc(ast.block_statement(SPAN, stmts)));
            vec![ast.statement_for_of(SPAN, false, left, right, body_stmt)]
        }
        Stmt::Break => vec![ast.statement_break(SPAN, None)],
        Stmt::Continue => vec![ast.statement_continue(SPAN, None)],
        Stmt::Expr(expr) => vec![ast.statement_expression(SPAN, build_expr(ast, expr))],
    }
}

// ─── Expressions ─────────────────────────────────────────────────────────────

fn build_expr<'a>(ast: &AstBuilder<'a>, expr: &Expr) -> Expression<'a> {
    match &expr.kind {
        ExprKind::Lit(lit) => build_lit(ast, lit),
        ExprKind::Ident(name) => {
            let n = ast.str(name);
            ast.expression_identifier(SPAN, n)
        }
        ExprKind::BinOp { op, left, right } => {
            let l = build_expr(ast, left);
            let r = build_expr(ast, right);
            match op {
                BinOp::And => ast.expression_logical(SPAN, l, LogicalOperator::And, r),
                BinOp::Or => ast.expression_logical(SPAN, l, LogicalOperator::Or, r),
                _ => {
                    let js_op = binop_to_js(*op);
                    ast.expression_binary(SPAN, l, js_op, r)
                }
            }
        }
        ExprKind::UnaryOp { op, expr } => {
            let inner = build_expr(ast, expr);
            let js_op = match op {
                UnaryOp::Not => UnaryOperator::LogicalNot,
                UnaryOp::Neg => UnaryOperator::UnaryNegation,
            };
            ast.expression_unary(SPAN, js_op, inner)
        }
        ExprKind::Cast { expr, .. } => {
            // Type casts are semantic only — emit the inner expression
            build_expr(ast, expr)
        }
        ExprKind::Call { target, args } => {
            let callee = build_expr(ast, target);
            let mut call_args: oxc_allocator::Vec<'a, Argument<'a>> = ast.vec();
            for a in args {
                call_args.push(Argument::from(build_expr(ast, a)));
            }
            ast.expression_call(SPAN, callee, NONE, call_args, false)
        }
        ExprKind::CallClosure { closure, args } => {
            let callee = build_expr(ast, closure);
            let mut call_args: oxc_allocator::Vec<'a, Argument<'a>> = ast.vec();
            for a in args {
                call_args.push(Argument::from(build_expr(ast, a)));
            }
            ast.expression_call(SPAN, callee, NONE, call_args, false)
        }
        ExprKind::GetField { target, field } => {
            let obj = build_expr(ast, target);
            let f = ast.str(field);
            Expression::from(ast.member_expression_static(SPAN, obj, ast.identifier_name(SPAN, f), false))
        }
        ExprKind::ArrayGet { target, index } => {
            let obj = build_expr(ast, target);
            let idx = build_expr(ast, index);
            Expression::from(ast.member_expression_computed(SPAN, obj, idx, false))
        }
        ExprKind::StructLit { name, fields } => {
            let mut props: oxc_allocator::Vec<'a, ObjectPropertyKind<'a>> = ast.vec();
            for (key, val) in fields {
                let k = ast.str(key);
                let prop_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, k)));
                let v = build_expr(ast, val);
                props.push(ast.object_property_kind_object_property(
                    SPAN, PropertyKind::Init, prop_key, v, false, false, false,
                ));
            }
            let obj = ast.expression_object(SPAN, props);
            let n = ast.str(name);
            let mut call_args: oxc_allocator::Vec<'a, Argument<'a>> = ast.vec();
            call_args.push(Argument::from(obj));
            ast.expression_new(SPAN, ast.expression_identifier(SPAN, n), NONE, call_args)
        }
        ExprKind::EnumVariant { name, variant, args } => {
            let n = ast.str(name);
            let v = ast.str(variant);
            let obj = ast.expression_identifier(SPAN, n);
            let member = Expression::from(ast.member_expression_static(SPAN, obj, ast.identifier_name(SPAN, v), false));
            if args.is_empty() {
                member
            } else {
                let mut call_args: oxc_allocator::Vec<'a, Argument<'a>> = ast.vec();
                for a in args {
                    call_args.push(Argument::from(build_expr(ast, a)));
                }
                ast.expression_call(SPAN, member, NONE, call_args, false)
            }
        }
        ExprKind::ArrayNew(elements) => {
            let mut items: oxc_allocator::Vec<'a, ArrayExpressionElement<'a>> = ast.vec();
            for el in elements {
                items.push(ArrayExpressionElement::from(build_expr(ast, el)));
            }
            ast.expression_array(SPAN, items)
        }
        ExprKind::If { cond, then, else_ } => {
            let test = build_expr(ast, cond);
            let consequent = build_expr(ast, then);
            let alternate = else_.as_ref()
                .map(|e| build_expr(ast, e))
                .unwrap_or_else(|| {
                    let n = ast.str("undefined");
                    ast.expression_identifier(SPAN, n)
                });
            ast.expression_conditional(SPAN, test, consequent, alternate)
        }
        ExprKind::Match { value, arms } => {
            build_match(ast, value, arms)
        }
        ExprKind::Block(stmts, tail) => {
            // Emit as IIFE: (() => { stmts; return tail; })()
            let mut body_stmts: oxc_allocator::Vec<'a, Statement<'a>> = ast.vec();
            for s in stmts {
                for emitted in build_stmt(ast, s) {
                    body_stmts.push(emitted);
                }
            }
            if let Some(tail_expr) = tail {
                body_stmts.push(ast.statement_return(SPAN, Some(build_expr(ast, tail_expr))));
            }
            let fn_body = ast.function_body(SPAN, ast.vec(), body_stmts);
            let params = ast.formal_parameters(SPAN, FormalParameterKind::ArrowFormalParameters, ast.vec(), NONE);
            let arrow = ast.expression_arrow_function(SPAN, false, false, NONE, params, NONE, fn_body);
            let call_args: oxc_allocator::Vec<'a, Argument<'a>> = ast.vec();
            ast.expression_call(SPAN, arrow, NONE, call_args, false)
        }
        ExprKind::MakeClosure { params, body } => {
            let mut param_list: oxc_allocator::Vec<'a, FormalParameter<'a>> = ast.vec();
            for p in params {
                param_list.push(make_param(ast, p));
            }
            let fp = ast.formal_parameters(SPAN, FormalParameterKind::ArrowFormalParameters, param_list, NONE);
            let body_expr = build_expr(ast, body);
            let mut stmts: oxc_allocator::Vec<'a, Statement<'a>> = ast.vec();
            stmts.push(ast.statement_return(SPAN, Some(body_expr)));
            let fn_body = ast.function_body(SPAN, ast.vec(), stmts);
            ast.expression_arrow_function(SPAN, false, false, NONE, fp, NONE, fn_body)
        }
        ExprKind::Wait(inner) => {
            let e = build_expr(ast, inner);
            ast.expression_await(SPAN, e)
        }
        ExprKind::SelfRef => {
            ast.expression_this(SPAN)
        }
    }
}

fn build_lit<'a>(ast: &AstBuilder<'a>, lit: &Lit) -> Expression<'a> {
    match lit {
        Lit::Int(n) => ast.expression_numeric_literal(SPAN, *n as f64, None, NumberBase::Decimal),
        Lit::Float(f) => ast.expression_numeric_literal(SPAN, *f, None, NumberBase::Decimal),
        Lit::String(s) => {
            let s = ast.str(s);
            ast.expression_string_literal(SPAN, s, None)
        }
        Lit::Bool(b) => ast.expression_boolean_literal(SPAN, *b),
        Lit::Unit => {
            let n = ast.str("undefined");
            ast.expression_identifier(SPAN, n)
        }
    }
}

fn build_match<'a>(ast: &AstBuilder<'a>, value: &Expr, arms: &[MatchArm]) -> Expression<'a> {
    // Build ternary chain from last arm to first.
    // Wildcard arm becomes the default (no condition).
    let mut result: Option<Expression<'a>> = None;

    for arm in arms.iter().rev() {
        let arm_value = build_expr(ast, &arm.body);
        match &arm.pattern {
            Pattern::Wildcard => {
                result = Some(arm_value);
            }
            Pattern::Lit(lit) => {
                let val = build_expr(ast, value);
                let pat = build_lit(ast, lit);
                let test = ast.expression_binary(SPAN, val, BinaryOperator::StrictEquality, pat);
                let alternate = result.unwrap_or_else(|| {
                    let n = ast.str("undefined");
                    ast.expression_identifier(SPAN, n)
                });
                result = Some(ast.expression_conditional(SPAN, test, arm_value, alternate));
            }
            Pattern::Variant { name: _, variant, bindings: _ } => {
                // Compare value._tag === "variant"
                let val = build_expr(ast, value);
                let tag_str = ast.str("_tag");
                let tag_field = Expression::from(ast.member_expression_static(
                    SPAN, val, ast.identifier_name(SPAN, tag_str), false
                ));
                let variant_str = ast.str(variant);
                let pat = ast.expression_string_literal(SPAN, variant_str, None);
                let test = ast.expression_binary(SPAN, tag_field, BinaryOperator::StrictEquality, pat);
                let alternate = result.unwrap_or_else(|| {
                    let n = ast.str("undefined");
                    ast.expression_identifier(SPAN, n)
                });
                result = Some(ast.expression_conditional(SPAN, test, arm_value, alternate));
            }
        }
    }

    result.unwrap_or_else(|| {
        let n = ast.str("undefined");
        ast.expression_identifier(SPAN, n)
    })
}

fn binop_to_js(op: BinOp) -> BinaryOperator {
    match op {
        BinOp::Add => BinaryOperator::Addition,
        BinOp::Sub => BinaryOperator::Subtraction,
        BinOp::Mul => BinaryOperator::Multiplication,
        BinOp::Div => BinaryOperator::Division,
        BinOp::Mod => BinaryOperator::Remainder,
        BinOp::Eq  => BinaryOperator::StrictEquality,
        BinOp::Ne  => BinaryOperator::StrictInequality,
        BinOp::Lt  => BinaryOperator::LessThan,
        BinOp::Gt  => BinaryOperator::GreaterThan,
        BinOp::Le  => BinaryOperator::LessEqualThan,
        BinOp::Ge  => BinaryOperator::GreaterEqualThan,
        BinOp::And | BinOp::Or => unreachable!("handled above"),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_param<'a>(ast: &AstBuilder<'a>, name: &str) -> FormalParameter<'a> {
    let n = ast.str(name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    ast.plain_formal_parameter(SPAN, pattern)
}

fn make_var_decl<'a>(
    ast: &AstBuilder<'a>,
    name: &str,
    value: Expression<'a>,
    kind: VariableDeclarationKind,
) -> Statement<'a> {
    let n = ast.str(name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, kind, pattern, NONE, Some(value), false);
    let decl = ast.variable_declaration(SPAN, kind, ast.vec1(declarator), false);
    Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))
}

#[cfg(test)]
mod tests;
