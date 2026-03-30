//! Contract codegen — emits Roca contracts as JS const objects and error maps.
//! Handles both enum-style value contracts and interface contracts with errors.

use crate::ast as roca;
use oxc_ast::ast::*;
use oxc_ast::AstBuilder;

use super::ast_helpers::{
    string_lit, number_lit, prop, object_expr, const_decl,
};

/// Emit a contract as:
/// - Enum contract -> const object
/// - Interface contract -> exported const with errors
pub(crate) fn build_contract_stmts<'a>(ast: &AstBuilder<'a>, c: &roca::ContractDef) -> Vec<Statement<'a>> {
    let mut stmts = Vec::new();

    // Enum-style contract: const StatusCode = { "200": 200, ... } as const;
    if !c.values.is_empty() {
        let mut props = ast.vec();
        for val in &c.values {
            match val {
                roca::ContractValue::Number(n) => {
                    let key_str = format!("{}", *n as i64);
                    props.push(prop(ast, &key_str, number_lit(ast, *n)));
                }
                roca::ContractValue::String(s) => {
                    props.push(prop(ast, s, string_lit(ast, s)));
                }
            }
        }
        let obj = object_expr(ast, props);
        stmts.push(const_decl(ast, &c.name, obj));
    }

    // Errors const: const HttpClientErrors = { timeout: "request timed out", ... };
    let all_errors: Vec<&roca::ErrDecl> = c.functions.iter().flat_map(|f| &f.errors).collect();
    if !all_errors.is_empty() {
        let err_name = format!("{}Errors", c.name);
        let mut props = ast.vec();
        for err in &all_errors {
            props.push(prop(ast, &err.name, string_lit(ast, &err.message)));
        }
        let obj = object_expr(ast, props);
        stmts.push(const_decl(ast, &err_name, obj));
    }

    stmts
}
