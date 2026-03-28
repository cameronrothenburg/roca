use super::harness::run;

// ─── Nullable fields parse correctly ────────────────────

#[test]
fn nullable_field_parses() {
    let file = roca::parse::parse(r#"
        pub struct User {
            name: String
            email: String | null
        }{}
    "#);
    if let roca::ast::Item::Struct(s) = &file.items[0] {
        assert_eq!(s.fields.len(), 2);
        assert_eq!(s.fields[0].type_ref, roca::ast::TypeRef::String);
        assert!(matches!(s.fields[1].type_ref, roca::ast::TypeRef::Nullable(_)));
    } else {
        panic!("expected struct");
    }
}

// ─── Nullable param — method call rejected ──────────────

#[test]
fn method_on_nullable_rejected() {
    let file = roca::parse::parse(r#"
        pub fn bad(name: String | null) -> String {
            return name.trim()
            crash { name.trim -> halt }
            test { self("hello") == "hello" }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "nullable-access"),
        "should catch method call on nullable, got: {:?}", errors);
}

#[test]
fn method_on_non_nullable_passes() {
    let file = roca::parse::parse(r#"
        pub fn ok(name: String) -> String {
            return name.trim()
            crash { name.trim -> halt }
            test { self("hello") == "hello" }
        }
    "#);
    let errors = roca::check::check(&file);
    let null_errors: Vec<_> = errors.iter().filter(|e| e.code == "nullable-access").collect();
    assert!(null_errors.is_empty(), "non-nullable should pass, got: {:?}", null_errors);
}

// ─── Nullable in struct — JS execution ──────────────────

#[test]
fn nullable_field_can_be_null() {
    assert_eq!(run(
        r#"
        pub struct Profile {
            name: String
            bio: String | null
        }{}

        pub fn has_bio(p: Profile) -> Bool {
            if p.bio == null { return false }
            return true
            test { self(Profile { name: "cam", bio: null }) == false }
        }
        "#,
        r#"
            const p1 = new Profile({ name: "cam", bio: null });
            console.log(has_bio(p1));
            const p2 = new Profile({ name: "cam", bio: "hello" });
            console.log(has_bio(p2));
        "#,
    ), "false\ntrue");
}

#[test]
fn nullable_field_with_value() {
    assert_eq!(run(
        r#"
        pub struct Config {
            name: String
            description: String | null
        }{}

        pub fn display(c: Config) -> String {
            if c.description == null { return c.name }
            return c.name + ": " + c.description
            test { self(Config { name: "app", description: null }) == "app" }
        }
        "#,
        r#"
            const c1 = new Config({ name: "app", description: null });
            console.log(display(c1));
            const c2 = new Config({ name: "app", description: "my app" });
            console.log(display(c2));
        "#,
    ), "app\napp: my app");
}

// ─── Return type nullable ───────────────────────────────

#[test]
fn function_returns_nullable() {
    assert_eq!(run(
        r#"
        pub fn find(id: String) -> String | null {
            if id == "" { return null }
            return "found: " + id
            test {
                self("1") == "found: 1"
                self("") == null
            }
        }
        "#,
        r#"
            console.log(find("1"));
            console.log(find(""));
        "#,
    ), "found: 1\nnull");
}

// ─── Error message quality ──────────────────────────────

#[test]
fn nullable_error_mentions_null_check() {
    let file = roca::parse::parse(r#"
        pub fn bad(s: String | null) -> String {
            return s.trim()
            crash { s.trim -> halt }
            test { self("a") == "a" }
        }
    "#);
    let errors = roca::check::check(&file);
    let err = errors.iter().find(|e| e.code == "nullable-access").unwrap();
    assert!(err.message.contains("null"), "error should mention null check");
    assert!(err.message.contains("trim"), "error should mention the method");
}
