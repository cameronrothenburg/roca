use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::ast::NumberBase;
use oxc_ast::NONE;
use oxc_ast::AstBuilder;
use oxc_span::SPAN;

/// Emit a contract as:
/// - Enum contract → const object
/// - Interface contract → exported const with errors
pub(crate) fn build_contract_stmts<'a>(ast: &AstBuilder<'a>, c: &roca::ContractDef) -> Vec<Statement<'a>> {
    let mut stmts = Vec::new();

    // Enum-style contract: const StatusCode = { "200": 200, ... } as const;
    if !c.values.is_empty() {
        let mut props = ast.vec();
        for val in &c.values {
            match val {
                roca::ContractValue::Number(n) => {
                    let key_str = format!("{}", *n as i64);
                    let k = ast.str(&key_str);
                    let prop_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, k)));
                    let value = ast.expression_numeric_literal(SPAN, *n, None, NumberBase::Decimal);
                    let prop = ast.object_property_kind_object_property(
                        SPAN, PropertyKind::Init, prop_key, value, false, false, false,
                    );
                    props.push(prop);
                }
                roca::ContractValue::String(s) => {
                    let k = ast.str(s);
                    let prop_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, k)));
                    let v = ast.str(s);
                    let value = ast.expression_string_literal(SPAN, v, None);
                    let prop = ast.object_property_kind_object_property(
                        SPAN, PropertyKind::Init, prop_key, value, false, false, false,
                    );
                    props.push(prop);
                }
            }
        }
        let obj = ast.expression_object(SPAN, props);
        let n = ast.str(&c.name);
        let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
        let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, pattern, NONE, Some(obj), false);
        let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
        stmts.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(decl))));
    }

    // Errors const: const HttpClientErrors = { timeout: "request timed out", ... };
    let all_errors: Vec<&roca::ErrDecl> = c.functions.iter().flat_map(|f| &f.errors).collect();
    if !all_errors.is_empty() {
        let err_name = format!("{}Errors", c.name);
        let mut props = ast.vec();
        for err in &all_errors {
            let k = ast.str(&err.name);
            let prop_key = PropertyKey::StaticIdentifier(ast.alloc(ast.identifier_name(SPAN, k)));
            let v = ast.str(&err.message);
            let value = ast.expression_string_literal(SPAN, v, None);
            let prop = ast.object_property(SPAN, PropertyKind::Init, prop_key, value, false, false, false);
            props.push(ObjectPropertyKind::ObjectProperty(ast.alloc(prop)));
        }
        let obj = ast.expression_object(SPAN, props);
        let n = ast.str(&err_name);
        let pattern = ast.binding_pattern_binding_identifier(SPAN, n);
        let declarator = ast.variable_declarator(SPAN, VariableDeclarationKind::Const, pattern, NONE, Some(obj), false);
        let decl = ast.variable_declaration(SPAN, VariableDeclarationKind::Const, ast.vec1(declarator), false);
        stmts.push(Statement::from(Declaration::VariableDeclaration(ast.alloc(decl))));
    }

    stmts
}
