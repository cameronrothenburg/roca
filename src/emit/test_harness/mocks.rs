use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use super::values::{emit_expr_js, mock_value_for_type};

/// Emit a mock object for a contract with a mock block.
/// Generates: const __mock_ContractName = { method() { return mockValue; }, ... };
pub(crate) fn emit_mock_object<'a>(
    ast: &AstBuilder<'a>,
    contract_name: &str,
    mock_def: &roca::MockDef,
    is_extern: bool,
    sigs: &[roca::FnSignature],
    body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
) {
    let mut props = ast.vec();

    for entry in &mock_def.entries {
        let value = crate::emit::expressions::build_expr(ast, &entry.value);

        // Only wrap in {value, err} if the method declares -> Type, err
        let method_returns_err = is_extern && sigs.iter()
            .find(|s| s.name == entry.method)
            .map(|s| s.returns_err)
            .unwrap_or(false);

        let return_val = if method_returns_err {
            // Extern mocks with errors: wrap in { value, err: null } — crash wrappers expect result objects
            let null_expr = ast.expression_null_literal(SPAN);
            let mut props = ast.vec();
            let val_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, "value")));
            props.push(ast.object_property_kind_object_property(SPAN, PropertyKind::Init, val_key, value, false, false, false));
            let err_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, "err")));
            props.push(ast.object_property_kind_object_property(SPAN, PropertyKind::Init, err_key, null_expr, false, false, false));
            ast.expression_object(SPAN, props)
        } else {
            value
        };

        let mut stmts = ast.vec();
        stmts.push(ast.statement_return(SPAN, Some(return_val)));
        let fn_body = ast.function_body(SPAN, ast.vec(), stmts);
        let formal_params = ast.formal_parameters(SPAN, FormalParameterKind::FormalParameter, ast.vec(), NONE);
        let func = ast.function(
            SPAN, FunctionType::FunctionExpression, None, false, false, false,
            NONE, NONE, formal_params, NONE, Some(fn_body),
        );

        let method_name = ast.str(&entry.method);
        let key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, method_name)));
        let method = ast.object_property_kind_object_property(
            SPAN, PropertyKind::Init, key,
            Expression::FunctionExpression(ast.alloc(func)),
            false, false, false,
        );
        props.push(method);
    }

    let obj = ast.expression_object(SPAN, props);
    let var_name = format!("__mock_{}", contract_name);
    let n = ast.str(&var_name);
    let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
    let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, pattern, NONE, Some(obj), false);
    let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
    body.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(decl))));
}

/// Generate JS code that patches struct/extern fn mocks for test isolation.
pub(crate) fn generate_mock_patches(file: &roca::SourceFile, is_embed: bool) -> String {
    let mut patches = Vec::new();

    if !is_embed {
        let mut structs: Vec<(&str, &[roca::Field], &[roca::FnSignature])> = Vec::new();
        for item in &file.items {
            if let roca::Item::Struct(s) = item {
                if !s.signatures.is_empty() {
                    structs.push((&s.name, &s.fields, &s.signatures));
                }
            }
        }

        for (name, fields, sigs) in &structs {
            for sig in *sigs {
                if sig.returns_err && !sig.errors.is_empty() {
                    let field_mocks: Vec<String> = fields.iter().map(|f| {
                        let mock_val = mock_value_for_type(&f.type_ref);
                        format!("{}: {}", f.name, mock_val)
                    }).collect();

                    let constructor_args = if field_mocks.is_empty() {
                        "{}".to_string()
                    } else {
                        format!("{{ {} }}", field_mocks.join(", "))
                    };

                    patches.push(format!(
                        "const _save_{name}_{method} = {name}.{method};\n\
                         {name}.{method} = function() {{ return {{ value: new {name}({args}), err: null }}; }};",
                        name = name,
                        method = sig.name,
                        args = constructor_args,
                    ));
                }
            }
        }
    }

    for item in &file.items {
        if let roca::Item::ExternFn(f) = item {
            if let Some(mock_def) = &f.mock {
                for entry in &mock_def.entries {
                    let mock_val = emit_expr_js(&entry.value);
                    patches.push(format!(
                        "globalThis.{name} = async function() {{ return {val}; }};",
                        name = f.name,
                        val = mock_val,
                    ));
                }
            }
        }
    }

    if patches.is_empty() {
        return String::new();
    }

    format!("// Auto-generated mock patches for dependency isolation\n{}", patches.join("\n"))
}
