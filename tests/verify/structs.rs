use super::harness::run;

#[test]
fn constructor_sets_fields() {
    assert_eq!(run(
        r#"pub struct Point {
            x: Number
            y: Number
        }{}"#,
        r#"
            const p = new Point({ x: 3, y: 4 });
            console.log(p.x);
            console.log(p.y);
        "#,
    ), "3\n4");
}

#[test]
fn constructor_single_field() {
    assert_eq!(run(
        r#"pub struct Name {
            value: String
        }{}"#,
        r#"
            const n = new Name({ value: "cam" });
            console.log(n.value);
        "#,
    ), "cam");
}

#[test]
fn static_method() {
    assert_eq!(run(
        r#"pub struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err missing = "required"
                err invalid = "invalid format"
            }
        }{
            fn validate(raw: String) -> Email, err {
                if raw == "" { return err.missing }
                return Email { value: raw }
                test {
                    self("a@b.com") is Ok
                    self("") is err.missing
                }
            }
        }"#,
        r#"
            const [email, err] = Email.validate("cam@test.com");
            console.log(email.value);
            console.log(err);
        "#,
    ), "cam@test.com\nnull");
}

#[test]
fn static_method_returns_error() {
    assert_eq!(run(
        r#"pub struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err missing = "required"
            }
        }{
            fn validate(raw: String) -> Email, err {
                if raw == "" { return err.missing }
                return Email { value: raw }
                test {
                    self("a@b.com") is Ok
                    self("") is err.missing
                }
            }
        }"#,
        r#"
            const [email, err] = Email.validate("");
            console.log(email === null);
            console.log(err.message);
        "#,
    ), "true\nmissing");
}

#[test]
fn multiple_fields_and_validate() {
    assert_eq!(run(
        r#"pub struct User {
            name: String
            age: Number
            validate(name: String, age: Number) -> User, err {
                err missing_name = "name required"
                err invalid_age = "age must be positive"
            }
        }{
            fn validate(name: String, age: Number) -> User, err {
                if name == "" { return err.missing_name }
                if age < 0 { return err.invalid_age }
                return User { name: name, age: age }
                test {
                    self("cam", 25) is Ok
                    self("", 25) is err.missing_name
                    self("cam", -1) is err.invalid_age
                }
            }
        }"#,
        r#"
            const [u, _e0] = User.validate("cam", 25);
            console.log(u.name);
            console.log(u.age);
            const [_v1, e1] = User.validate("", 25);
            console.log(e1.message);
            const [_v2, e2] = User.validate("cam", -1);
            console.log(e2.message);
        "#,
    ), "cam\n25\nmissing_name\ninvalid_age");
}

#[test]
fn empty_struct_no_fields() {
    assert_eq!(run(
        r#"pub struct Config {
            get_default() -> Number
        }{
            fn get_default() -> Number {
                return 42
                test { self() == 42 }
            }
        }"#,
        "console.log(Config.get_default());",
    ), "42");
}
