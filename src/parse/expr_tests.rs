use super::*;
use crate::parse::tokenize;
use crate::ast::{Expr, BinOp, StringPart};

#[test]
fn parse_simple_string() {
    let mut p = Parser::new(tokenize("\"hello\""));
    assert_eq!(p.parse_expr().unwrap(), Expr::String("hello".into()));
}

#[test]
fn parse_binop() {
    let mut p = Parser::new(tokenize("1 + 2"));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::BinOp { op: BinOp::Add, .. }));
}

#[test]
fn parse_field_access() {
    let mut p = Parser::new(tokenize("user.name"));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::FieldAccess { field, .. } if field == "name"));
}

#[test]
fn parse_method_call() {
    let mut p = Parser::new(tokenize("name.trim()"));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::Call { .. }));
}

#[test]
fn parse_err_as_ident() {
    // err.X is now parsed as field access on the err variable
    // ErrRef is only used via ReturnErr in statement parser
    let mut p = Parser::new(tokenize("err.timeout"));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::FieldAccess { field, .. } if field == "timeout"));
}

#[test]
fn parse_struct_literal() {
    let mut p = Parser::new(tokenize("Email { value: \"test\" }"));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::StructLit { name, .. } if name == "Email"));
}

#[test]
fn parse_function_call() {
    let mut p = Parser::new(tokenize("greet(\"cam\")"));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::Call { .. }));
}

#[test]
fn parse_chained_method() {
    let mut p = Parser::new(tokenize("raw.trim().to_upper()"));
    let expr = p.parse_expr().unwrap();
    // Should be Call(FieldAccess(Call(FieldAccess(Ident(raw), trim)), to_upper))
    assert!(matches!(expr, Expr::Call { .. }));
}

#[test]
fn parse_error_on_bad_token() {
    let mut p = Parser::new(tokenize("->"));
    let result = p.parse_expr();
    assert!(result.is_err());
}

// ─── String interpolation edge cases ─────

#[test]
fn json_string_not_interpolated() {
    // "{key: value}" should be a plain string, not interpolation
    let mut p = Parser::new(tokenize(r#""{key: value}""#));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(&expr, Expr::String(s) if s == "{key: value}"),
        "JSON-like string should not be interpolated, got: {:?}", expr);
}

#[test]
fn json_object_string_not_interpolated() {
    let mut p = Parser::new(tokenize(r#""{"name":"cam"}""#));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::String(_)),
        "JSON object string should not be interpolated, got: {:?}", expr);
}

#[test]
fn empty_braces_not_interpolated() {
    let mut p = Parser::new(tokenize(r#""{}""#));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(&expr, Expr::String(s) if s == "{}"),
        "empty braces should not be interpolated, got: {:?}", expr);
}

#[test]
fn valid_interpolation_works() {
    let mut p = Parser::new(tokenize(r#""hello {name}""#));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::StringInterp(_)),
        "valid interpolation should work, got: {:?}", expr);
}

#[test]
fn dotted_interpolation_works() {
    let mut p = Parser::new(tokenize(r#""age is {user.age}""#));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::StringInterp(_)),
        "dotted interpolation should work, got: {:?}", expr);
}

#[test]
fn braces_with_spaces_not_interpolated() {
    let mut p = Parser::new(tokenize(r#""{ not valid }""#));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::String(_)),
        "braces with spaces around content should not interpolate, got: {:?}", expr);
}

#[test]
fn numeric_braces_not_interpolated() {
    let mut p = Parser::new(tokenize(r#""{123}""#));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::String(_)),
        "numeric braces should not be interpolated, got: {:?}", expr);
}

// ─── Escaped braces ─────

#[test]
fn escaped_braces_literal() {
    let mut p = Parser::new(tokenize(r#""\{name\}""#));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(&expr, Expr::String(s) if s == "{name}"),
        "escaped braces should produce literal string, got: {:?}", expr);
}

#[test]
fn escaped_brace_in_interpolated_string() {
    // \{literal\} followed by real {interp}
    let mut p = Parser::new(tokenize(r#""price: \{10\} for {name}""#));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::StringInterp(_)),
        "mixed escaped + real interpolation should work, got: {:?}", expr);
    if let Expr::StringInterp(parts) = expr {
        // First part should be literal "price: {10} for "
        if let StringPart::Literal(s) = &parts[0] {
            assert!(s.contains("{10}"), "escaped brace should be literal, got: {}", s);
        }
    }
}

#[test]
fn only_escaped_braces_no_interpolation() {
    let mut p = Parser::new(tokenize(r#""\{hello\}""#));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(&expr, Expr::String(s) if s == "{hello}"),
        "all-escaped braces should be plain string, got: {:?}", expr);
}

#[test]
fn css_string_not_interpolated() {
    let mut p = Parser::new(tokenize(r#"".class { color: red; }""#));
    let expr = p.parse_expr().unwrap();
    assert!(matches!(expr, Expr::String(_)),
        "CSS-like string should not be interpolated, got: {:?}", expr);
}
