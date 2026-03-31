//! Value rendering utilities for test emission.
//! Converts Roca expressions and types into JS string representations for mock values.

use crate::ast as roca;

/// Render a Roca expression as a JS string (for mock values)
pub(crate) fn emit_expr_js(expr: &roca::Expr) -> String {
    match expr {
        roca::Expr::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        roca::Expr::Number(n) => {
            if *n == (*n as i64) as f64 { format!("{}", *n as i64) } else { format!("{}", n) }
        }
        roca::Expr::Bool(b) => format!("{}", b),
        roca::Expr::Null => "null".to_string(),
        roca::Expr::Ident(name) => {
            if name == "Ok" { "null".to_string() } else { name.clone() }
        }
        roca::Expr::StructLit { name: _, fields } => {
            let props: Vec<String> = fields.iter()
                .map(|(k, v)| format!("{}: {}", k, emit_expr_js(v)))
                .collect();
            format!("{{ {} }}", props.join(", "))
        }
        roca::Expr::Array(elements) => {
            let items: Vec<String> = elements.iter().map(|e| emit_expr_js(e)).collect();
            format!("[{}]", items.join(", "))
        }
        _ => "null".to_string(),
    }
}

pub(crate) fn mock_value_for_type(t: &roca::TypeRef) -> String {
    match t {
        roca::TypeRef::String => "\"mock_\" + Math.random().toString(36).slice(2)".to_string(),
        roca::TypeRef::Number => "Math.floor(Math.random() * 100)".to_string(),
        roca::TypeRef::Bool => "true".to_string(),
        roca::TypeRef::Named(name) => format!("new {}({{}})", name),
        _ => "null".to_string(),
    }
}

/// Generate a default Roca expression for a type — used for auto-stubs.
pub(crate) fn default_expr_for_type(ty: &roca::TypeRef) -> roca::Expr {
    match ty {
        roca::TypeRef::String => roca::Expr::String("".into()),
        roca::TypeRef::Number => roca::Expr::Number(0.0),
        roca::TypeRef::Bool => roca::Expr::Bool(false),
        roca::TypeRef::Ok => roca::Expr::Null,
        roca::TypeRef::Named(_) => roca::Expr::Null,
        roca::TypeRef::Generic(name, _) if name == "Array" => roca::Expr::Array(vec![]),
        _ => roca::Expr::Null,
    }
}

/// Auto-generate a MockDef from contract/extern fn signatures.
pub(crate) fn auto_mock_def(sigs: &[roca::FnSignature]) -> roca::MockDef {
    roca::MockDef {
        entries: sigs.iter().map(|s| roca::MockEntry {
            method: s.name.clone(),
            value: default_expr_for_type(&s.return_type),
        }).collect(),
    }
}

/// Auto-generate a MockDef for a single extern fn.
pub(crate) fn auto_mock_def_for_extern_fn(ef: &roca::ExternFnDef) -> roca::MockDef {
    roca::MockDef {
        entries: vec![roca::MockEntry {
            method: ef.name.clone(),
            value: default_expr_for_type(&ef.return_type),
        }],
    }
}
