use super::harness::run;

// ─── Parsing ────────────────────────────────────────────

#[test]
fn string_constraints_parse() {
    let file = roca::parse::parse(r#"
        pub struct User {
            name: String { min: 1, max: 64 }
            email: String { contains: "@", min: 3 }
        }{}
    "#);
    if let roca::ast::Item::Struct(s) = &file.items[0] {
        assert_eq!(s.fields[0].constraints.len(), 2);
        assert_eq!(s.fields[1].constraints.len(), 2);
    }
}

#[test]
fn number_constraints_parse() {
    let file = roca::parse::parse(r#"
        pub struct Config {
            timeout: Number { min: 0, max: 30000 }
            retries: Number { min: 1, max: 10 }
        }{}
    "#);
    if let roca::ast::Item::Struct(s) = &file.items[0] {
        assert_eq!(s.fields[0].constraints.len(), 2);
        assert!(matches!(s.fields[0].constraints[0], roca::ast::Constraint::Min(0.0)));
        assert!(matches!(s.fields[0].constraints[1], roca::ast::Constraint::Max(30000.0)));
    }
}

#[test]
fn no_constraints_is_empty() {
    let file = roca::parse::parse(r#"
        pub struct Simple { name: String }{}
    "#);
    if let roca::ast::Item::Struct(s) = &file.items[0] {
        assert!(s.fields[0].constraints.is_empty());
    }
}

#[test]
fn contract_fields_with_constraints() {
    let file = roca::parse::parse(r#"
        contract UserInput {
            name: String { min: 1, max: 100 }
            age: Number { min: 0, max: 150 }
        }
    "#);
    if let roca::ast::Item::Contract(c) = &file.items[0] {
        assert_eq!(c.fields[0].constraints.len(), 2);
        assert_eq!(c.fields[1].constraints.len(), 2);
    }
}

// ─── Checker ────────────────────────────────────────────

#[test]
fn min_greater_than_max_caught() {
    let file = roca::parse::parse(r#"
        pub struct Bad { age: Number { min: 150, max: 0 } }{}
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "invalid-constraint" && e.message.contains("min")),
        "should catch min > max, got: {:?}", errors);
}

#[test]
fn contains_on_number_caught() {
    let file = roca::parse::parse(r#"
        pub struct Bad { count: Number { contains: "x" } }{}
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "invalid-constraint"),
        "should catch contains on Number, got: {:?}", errors);
}

#[test]
fn valid_constraints_pass() {
    let file = roca::parse::parse(r#"
        pub struct User {
            name: String { min: 1, max: 64 }
            email: String { contains: "@" }
            age: Number { min: 0, max: 150 }
        }{}
    "#);
    let errors = roca::check::check(&file);
    let constraint_errors: Vec<_> = errors.iter().filter(|e| e.code == "invalid-constraint").collect();
    assert!(constraint_errors.is_empty(), "valid constraints should pass, got: {:?}", constraint_errors);
}

// ─── Mixed with other features ──────────────────────────

#[test]
fn constraints_with_nullable() {
    let file = roca::parse::parse(r#"
        pub struct Profile {
            name: String { min: 1, max: 64 }
            bio: String | null
        }{}
    "#);
    if let roca::ast::Item::Struct(s) = &file.items[0] {
        assert_eq!(s.fields[0].constraints.len(), 2);
        assert!(s.fields[1].constraints.is_empty());
        assert!(matches!(s.fields[1].type_ref, roca::ast::TypeRef::Nullable(_)));
    }
}

#[test]
fn constraints_with_pattern() {
    let file = roca::parse::parse(r#"
        pub struct Username {
            value: String { min: 3, max: 32, pattern: "[a-zA-Z0-9_]" }
        }{}
    "#);
    if let roca::ast::Item::Struct(s) = &file.items[0] {
        assert_eq!(s.fields[0].constraints.len(), 3);
    }
}
