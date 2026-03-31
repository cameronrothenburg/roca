//! Mock object emission — generates `__mock_ContractName` objects from Roca `mock` blocks.
//! Patches contract dependencies in tests with user-defined return values.

use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

use crate::emit::ast_helpers::{
    null_lit, prop, prop_key, object_expr, const_decl,
    return_stmt, formal_params, function_body, function_expr,
};
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
            let mut result_props = ast.vec();
            result_props.push(prop(ast, "value", value));
            result_props.push(prop(ast, "err", null_lit(ast)));
            object_expr(ast, result_props)
        } else {
            value
        };

        let mut stmts = ast.vec();
        stmts.push(return_stmt(ast, return_val));
        let fn_body = function_body(ast, stmts);
        let fn_params = formal_params(ast, ast.vec());
        let func_expr = function_expr(ast, fn_params, fn_body, false);

        let key = prop_key(ast, &entry.method);
        let method = ast.object_property_kind_object_property(
            SPAN, PropertyKind::Init, key,
            func_expr,
            false, false, false,
        );
        props.push(method);
    }

    let obj = object_expr(ast, props);
    let var_name = format!("__mock_{}", contract_name);
    body.push(const_decl(ast, &var_name, obj));
}

/// Generate JS code that patches struct/extern fn mocks for test isolation.
pub(crate) fn generate_mock_patches(files: &[&roca::SourceFile], is_embed: bool) -> String {
    let mut patches = Vec::new();

    if !is_embed {
        for file in files {
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
    }

    for file in files {
    for item in &file.items {
        match item {
            roca::Item::ExternFn(f) => {
                let mock_def = super::values::auto_mock_def_for_extern_fn(f);
                for entry in &mock_def.entries {
                    let mock_val = emit_expr_js(&entry.value);
                    let return_val = if f.returns_err {
                        format!("{{ value: {}, err: null }}", mock_val)
                    } else {
                        mock_val
                    };
                    patches.push(format!(
                        "globalThis.{name} = function() {{ return {val}; }};",
                        name = f.name,
                        val = return_val,
                    ));
                }
            }
            roca::Item::ExternContract(_) => {
                // Extern contracts have JS wrappers — let real implementations run.
                // Crash blocks handle failures (e.g., Fs.readFile in V8 without Node).
            }
            _ => {}
        }
    }
    }

    if patches.is_empty() {
        return String::new();
    }

    format!("// Auto-generated mock patches for dependency isolation\n{}", patches.join("\n"))
}
