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
