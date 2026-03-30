use super::harness::run;

#[test]
fn string_enum_parses() {
    let file = roca::parse::parse(r#"
        enum Status {
            active = "active"
            inactive = "inactive"
        }
    "#);
    if let roca::ast::Item::Enum(e) = &file.items[0] {
        assert_eq!(e.name, "Status");
        assert_eq!(e.variants.len(), 2);
        assert_eq!(e.variants[0].name, "active");
    }
}

#[test]
fn number_enum_parses() {
    let file = roca::parse::parse(r#"
        enum HttpCode {
            ok = 200
            not_found = 404
            server_error = 500
        }
    "#);
    if let roca::ast::Item::Enum(e) = &file.items[0] {
        assert_eq!(e.variants.len(), 3);
        assert!(matches!(e.variants[0].value, roca::ast::EnumValue::Number(200.0)));
    }
}

#[test]
fn enum_emits_js_object() {
    assert_eq!(run(
        r#"
        pub enum Status {
            active = "active"
            inactive = "inactive"
            suspended = "suspended"
        }

        pub fn check(s: String) -> String {
            if s == Status.active { return "is active" }
            return "not active"
            test { self("active") == "is active" }
        }
        "#,
        r#"
            console.log(Status.active);
            console.log(Status.suspended);
            console.log(check("active"));
            console.log(check("other"));
        "#,
    ), "active\nsuspended\nis active\nnot active");
}

#[test]
fn number_enum_emits_js() {
    assert_eq!(run(
        r#"
        pub enum HttpCode {
            ok = 200
            not_found = 404
            server_error = 500
        }

        pub fn is_ok(code: Number) -> Bool {
            return code == HttpCode.ok
            test { self(200) == true self(404) == false }
        }
        "#,
        r#"
            console.log(HttpCode.ok);
            console.log(HttpCode.not_found);
            console.log(is_ok(200));
            console.log(is_ok(404));
        "#,
    ), "200\n404\ntrue\nfalse");
}

#[test]
fn enum_in_match() {
    assert_eq!(run(
        r#"
        pub enum Color {
            red = "red"
            green = "green"
            blue = "blue"
        }

        pub fn describe(c: String) -> String {
            return match c {
                Color.red => "warm"
                Color.blue => "cool"
                _ => "other"
            }
            test { self("red") == "warm" self("blue") == "cool" self("green") == "other" }
        }
        "#,
        r#"
            console.log(describe("red"));
            console.log(describe("blue"));
            console.log(describe("green"));
        "#,
    ), "warm\ncool\nother");
}

#[test]
fn pub_enum_exported() {
    let file = roca::parse::parse(r#"
        pub enum Direction { up = "up" down = "down" }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("export"), "pub enum should be exported");
    assert!(js.contains("Direction"));
}

#[test]
fn private_enum_not_exported() {
    let file = roca::parse::parse(r#"
        enum Internal { a = "a" b = "b" }
    "#);
    let js = roca::emit::emit(&file);
    assert!(!js.contains("export"), "private enum should not be exported");
}

#[test]
fn enum_in_struct_field() {
    assert_eq!(run(
        r#"
        pub enum Role {
            admin = "admin"
            user = "user"
        }

        pub struct Account {
            name: String
            role: String
        }{}

        pub fn is_admin(a: Account) -> Bool {
            return a.role == Role.admin
            test { self(Account { name: "cam", role: "admin" }) == true }
        }
        "#,
        r#"
            const a = new Account({ name: "cam", role: "admin" });
            console.log(is_admin(a));
            const b = new Account({ name: "cam", role: "user" });
            console.log(is_admin(b));
        "#,
    ), "true\nfalse");
}
