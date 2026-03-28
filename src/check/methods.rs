use crate::ast::*;
use crate::errors::RuleError;
use super::registry::ContractRegistry;

/// Validate that method calls on known types exist in their contracts
pub fn check_methods(file: &SourceFile, registry: &ContractRegistry) -> Vec<RuleError> {
    let mut errors = Vec::new();

    for item in &file.items {
        match item {
            Item::Function(f) => {
                let scope = build_scope(&f.params);
                check_stmts(&f.body, &scope, registry, &f.name, &mut errors);
            }
            Item::Struct(s) => {
                for method in &s.methods {
                    let mut scope = build_scope(&method.params);
                    // Add struct fields as self.field types
                    for field in &s.fields {
                        scope.insert(
                            format!("self.{}", field.name),
                            type_ref_to_name(&field.type_ref),
                        );
                    }
                    let ctx = format!("{}.{}", s.name, method.name);
                    check_stmts(&method.body, &scope, registry, &ctx, &mut errors);
                }
            }
            Item::Satisfies(sat) => {
                for method in &sat.methods {
                    let scope = build_scope(&method.params);
                    let ctx = format!("{}.{}", sat.struct_name, method.name);
                    check_stmts(&method.body, &scope, registry, &ctx, &mut errors);
                }
            }
            _ => {}
        }
    }

    errors
}

/// Map of variable name -> type name
type Scope = std::collections::HashMap<String, String>;

fn build_scope(params: &[Param]) -> Scope {
    let mut scope = Scope::new();
    for p in params {
        scope.insert(p.name.clone(), type_ref_to_name(&p.type_ref));
    }
    scope
}

fn type_ref_to_name(t: &TypeRef) -> String {
    match t {
        TypeRef::String => "String".to_string(),
        TypeRef::Number => "Number".to_string(),
        TypeRef::Bool => "Bool".to_string(),
        TypeRef::Named(n) => n.clone(),
        TypeRef::Result(inner) => type_ref_to_name(inner),
        TypeRef::Ok => "Ok".to_string(),
    }
}

fn check_stmts(stmts: &[Stmt], scope: &Scope, registry: &ContractRegistry, ctx: &str, errors: &mut Vec<RuleError>) {
    let mut scope = scope.clone();

    for stmt in stmts {
        match stmt {
            Stmt::Const { name, value, type_ann, .. } => {
                check_expr(value, &scope, registry, ctx, errors);
                // Infer type from value and add to scope
                if let Some(t) = type_ann {
                    scope.insert(name.clone(), type_ref_to_name(t));
                } else if let Some(t) = infer_type(value, &scope) {
                    scope.insert(name.clone(), t);
                }
            }
            Stmt::Let { name, value, type_ann, .. } => {
                check_expr(value, &scope, registry, ctx, errors);
                if let Some(t) = type_ann {
                    scope.insert(name.clone(), type_ref_to_name(t));
                } else if let Some(t) = infer_type(value, &scope) {
                    scope.insert(name.clone(), t);
                }
            }
            Stmt::LetResult { name, value, .. } => {
                check_expr(value, &scope, registry, ctx, errors);
            }
            Stmt::Return(expr) | Stmt::Expr(expr) => {
                check_expr(expr, &scope, registry, ctx, errors);
            }
            Stmt::Assign { value, .. } => {
                check_expr(value, &scope, registry, ctx, errors);
            }
            Stmt::ReturnErr(_) => {}
            Stmt::If { condition, then_body, else_body } => {
                check_expr(condition, &scope, registry, ctx, errors);
                check_stmts(then_body, &scope, registry, ctx, errors);
                if let Some(body) = else_body {
                    check_stmts(body, &scope, registry, ctx, errors);
                }
            }
            Stmt::For { iter, body, .. } => {
                check_expr(iter, &scope, registry, ctx, errors);
                check_stmts(body, &scope, registry, ctx, errors);
            }
        }
    }
}

fn check_expr(expr: &Expr, scope: &Scope, registry: &ContractRegistry, ctx: &str, errors: &mut Vec<RuleError>) {
    match expr {
        Expr::Call { target, args } => {
            // Check if the call target is a method on a known type
            if let Expr::FieldAccess { target: obj, field } = target.as_ref() {
                if let Some(type_name) = resolve_type(obj, scope) {
                    if !registry.has_method(&type_name, field) {
                        let available = registry.available_methods(&type_name);
                        let hint = if available.is_empty() {
                            String::new()
                        } else {
                            format!("\n  available: {}", available.join(", "))
                        };
                        errors.push(RuleError {
                            code: "unknown-method".into(),
                            message: format!(
                                "'{}' has no method '{}'{}",
                                type_name, field, hint
                            ),
                            context: Some(ctx.to_string()),
                        });
                    }
                }
            }
            // Check args recursively
            for a in args {
                check_expr(a, scope, registry, ctx, errors);
            }
            check_expr(target, scope, registry, ctx, errors);
        }
        Expr::FieldAccess { target, field } => {
            // Field access on known type — check if field exists
            // Only check if it's NOT also a call target (handled above)
            check_expr(target, scope, registry, ctx, errors);
        }
        Expr::BinOp { left, right, .. } => {
            check_expr(left, scope, registry, ctx, errors);
            check_expr(right, scope, registry, ctx, errors);
        }
        Expr::StructLit { fields, .. } => {
            for (_, v) in fields {
                check_expr(v, scope, registry, ctx, errors);
            }
        }
        Expr::Array(elements) => {
            for e in elements {
                check_expr(e, scope, registry, ctx, errors);
            }
        }
        Expr::Index { target, index } => {
            check_expr(target, scope, registry, ctx, errors);
            check_expr(index, scope, registry, ctx, errors);
        }
        Expr::Match { value, arms } => {
            check_expr(value, scope, registry, ctx, errors);
            for arm in arms {
                if let Some(p) = &arm.pattern { check_expr(p, scope, registry, ctx, errors); }
                check_expr(&arm.value, scope, registry, ctx, errors);
            }
        }
        _ => {}
    }
}

/// Resolve the type of an expression from scope
fn resolve_type(expr: &Expr, scope: &Scope) -> Option<String> {
    match expr {
        Expr::Ident(name) => scope.get(name).cloned(),
        Expr::String(_) => Some("String".to_string()),
        Expr::Number(_) => Some("Number".to_string()),
        Expr::Bool(_) => Some("Bool".to_string()),
        Expr::Array(_) => Some("Array".to_string()),
        Expr::SelfRef => None, // self type needs struct context — skip for now
        _ => None,
    }
}

/// Infer the type of an expression
fn infer_type(expr: &Expr, scope: &Scope) -> Option<String> {
    match expr {
        Expr::String(_) => Some("String".to_string()),
        Expr::Number(_) => Some("Number".to_string()),
        Expr::Bool(_) => Some("Bool".to_string()),
        Expr::Array(_) => Some("Array".to_string()),
        Expr::Ident(name) => scope.get(name).cloned(),
        Expr::Call { target, .. } => {
            // If calling a method like name.trim(), result type is String
            if let Expr::FieldAccess { target: obj, field } = target.as_ref() {
                if let Some(type_name) = resolve_type(obj, scope) {
                    if type_name == "String" {
                        // Most string methods return String
                        return match field.as_str() {
                            "includes" | "startsWith" | "endsWith" => Some("Bool".to_string()),
                            "indexOf" => Some("Number".to_string()),
                            "split" => Some("Array".to_string()),
                            _ => Some("String".to_string()),
                        };
                    }
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use crate::check::registry::ContractRegistry;

    #[test]
    fn valid_string_method() {
        let file = parse::parse(r#"
            pub fn process(name: String) -> String {
                return name.trim()
                crash { name.trim -> halt }
                test { self("a") == "a" }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        let errors = check_methods(&file, &reg);
        assert!(errors.is_empty());
    }

    #[test]
    fn invalid_string_method() {
        let file = parse::parse(r#"
            pub fn process(name: String) -> String {
                return name.nonexistent()
                crash { name.nonexistent -> halt }
                test { self("a") == "a" }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        let errors = check_methods(&file, &reg);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "unknown-method");
        assert!(errors[0].message.contains("nonexistent"));
        assert!(errors[0].message.contains("available:"));
    }

    #[test]
    fn valid_number_method() {
        let file = parse::parse(r#"
            pub fn show(n: Number) -> String {
                return n.toString()
                crash { n.toString -> halt }
                test { self(42) == "42" }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        let errors = check_methods(&file, &reg);
        assert!(errors.is_empty());
    }

    #[test]
    fn number_cant_trim() {
        let file = parse::parse(r#"
            pub fn bad(n: Number) -> String {
                return n.trim()
                crash { n.trim -> halt }
                test { self(42) == "42" }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        let errors = check_methods(&file, &reg);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("Number"));
        assert!(errors[0].message.contains("trim"));
    }

    #[test]
    fn inferred_type_from_literal() {
        let file = parse::parse(r#"
            pub fn bad() -> String {
                const name = "hello"
                return name.fakefn()
                crash { name.fakefn -> halt }
                test { self() == "hello" }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        let errors = check_methods(&file, &reg);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("fakefn"));
    }

    #[test]
    fn chained_valid_methods() {
        let file = parse::parse(r#"
            pub fn process(s: String) -> String {
                const trimmed = s.trim()
                const upper = trimmed.toUpperCase()
                return upper
                crash {
                    s.trim -> halt
                    trimmed.toUpperCase -> halt
                }
                test { self("a") == "A" }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        let errors = check_methods(&file, &reg);
        assert!(errors.is_empty());
    }

    #[test]
    fn array_valid_methods() {
        let file = parse::parse(r#"
            pub fn test_arr() -> String {
                const arr = ["a", "b"]
                return arr.join(",")
                crash { arr.join -> halt }
                test { self() == "a,b" }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        let errors = check_methods(&file, &reg);
        assert!(errors.is_empty());
    }

    #[test]
    fn array_invalid_method() {
        let file = parse::parse(r#"
            pub fn test_arr() -> String {
                const arr = ["a", "b"]
                return arr.trim()
                crash { arr.trim -> halt }
                test { self() == "a,b" }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        let errors = check_methods(&file, &reg);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("Array"));
        assert!(errors[0].message.contains("trim"));
    }
}
