use super::harness::run;

// ─── Full pipeline: contract + struct + satisfies + function ─────

#[test]
fn full_email_pipeline() {
    // Contract defines what, struct implements how, satisfies links them,
    // function uses the struct — the complete Roca flow
    assert_eq!(run(
        r#"
        contract Stringable { to_string() -> String }

        pub struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err missing = "value is required"
                err invalid = "format is not valid"
            }
        }{
            pub fn validate(raw: String) -> Email, err {
                if raw == "" { return err.missing }
                return Email { value: raw }
                test {
                    self("a@b.com") is Ok
                    self("") is err.missing
                    self("x") is err.invalid
                }
            }
        }

        Email satisfies Stringable {
            fn to_string() -> String {
                return self.value
                test { self() == "a@b.com" }
            }
        }

        pub fn format_email(raw: String) -> String {
            const result = Email.validate(raw)
            return "ok"
            crash { Email.validate -> fallback(fn(e) -> "error") }
            test {
                self("a@b.com") == "ok"
            }
        }
        "#,
        r#"
            // Validate and use
            const { value: email, err } = Email.validate("cam@test.com");
            console.log(email.value);
            console.log(email.to_string());
            console.log(err);

            // Validate failure
            const { value: bad, err: err2 } = Email.validate("");
            console.log(bad);
            console.log(err2.name);
            console.log(err2.message);

            // Contract errors accessible
            console.log(typeof EmailErrors === "undefined");
        "#,
    ), "cam@test.com\ncam@test.com\nnull\nnull\nmissing\nvalue is required\ntrue");
    // Note: EmailErrors is undefined because errors are on the struct, not a separate contract
}

#[test]
fn full_user_registration() {
    assert_eq!(run(
        r#"
        contract Stringable { to_string() -> String }

        pub struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err invalid = "invalid email"
            }
        }{
            pub fn validate(raw: String) -> Email, err {
                if raw == "" { return err.invalid }
                return Email { value: raw }
                test {
                    self("a@b.com") is Ok
                    self("") is err.invalid
                }
            }
        }

        Email satisfies Stringable {
            fn to_string() -> String {
                return self.value
                test { self() == "test" }
            }
        }

        pub struct User {
            name: String
            email: Email
            validate(name: String, email_raw: String) -> User, err {
                err missing_name = "name is required"
                err bad_email = "email is invalid"
            }
        }{
            pub fn validate(name: String, email_raw: String) -> User, err {
                if name == "" { return err.missing_name }
                if email_raw == "" { return err.bad_email }
                const email_result = Email.validate(email_raw)
                return User { name: name, email: email_result }
                crash { Email.validate -> skip }
                test {
                    self("cam", "a@b.com") is Ok
                    self("", "a@b.com") is err.missing_name
                    self("cam", "") is err.bad_email
                }
            }
        }

        User satisfies Stringable {
            fn to_string() -> String {
                return self.name
                test { self() == "cam" }
            }
        }
        "#,
        r#"
            // Note: Email.validate returns { value: Email, err: null } object
            // but User.validate uses it directly — crash halt would throw on error
            // For this test, we call with valid data
            const { value: emailResult, err: _e0 } = Email.validate("cam@test.com");
            const { value: user, err: _e1 } = User.validate("cam", "cam@test.com");

            console.log(user.name);
            console.log(user.to_string());

            // Test error path
            const { value: bad, err } = User.validate("", "a@b.com");
            console.log(bad);
            console.log(err.name);
        "#,
    ), "cam\ncam\nnull\nmissing_name");
}

// ─── Struct calling struct ──────────────────────────────

#[test]
fn struct_uses_another_struct() {
    assert_eq!(run(
        r#"
        pub struct Point {
            x: Number
            y: Number
            create(x: Number, y: Number) -> Point
        }{
            pub fn create(x: Number, y: Number) -> Point {
                return Point { x: x, y: y }
                test { self(1, 2) == Point { x: 1, y: 2 } }
            }
        }

        pub struct Line {
            start: Point
            end: Point
            create(x1: Number, y1: Number, x2: Number, y2: Number) -> Line
        }{
            pub fn create(x1: Number, y1: Number, x2: Number, y2: Number) -> Line {
                const s = Point.create(x1, y1)
                const e = Point.create(x2, y2)
                return Line { start: s, end: e }
                crash {
                    Point.create -> skip
                }
                test { self(0, 0, 1, 1) == Line { start: Point { x: 0, y: 0 }, end: Point { x: 1, y: 1 } } }
            }
        }
        "#,
        r#"
            const line = Line.create(0, 0, 10, 20);
            console.log(line.start.x);
            console.log(line.start.y);
            console.log(line.end.x);
            console.log(line.end.y);
        "#,
    ), "0\n0\n10\n20");
}

// ─── Let result destructuring ───────────────────────────

#[test]
fn let_result_destructure() {
    assert_eq!(run(
        r#"
        /// Safe division
        pub struct SafeDivide {
            call(a: Number, b: Number) -> Number, err {
                err div_zero = "div_zero"
            }
        }{
            pub fn call(a: Number, b: Number) -> Number, err {
                if b == 0 { return err.div_zero }
                return a / b
                test {
                    self(10, 2) == 5
                    self(10, 0) is err.div_zero
                }
            }
        }

        /// Computes division result
        pub fn compute(a: Number, b: Number) -> String {
            const result = SafeDivide.call(a, b)
            return "result: " + result
            crash { SafeDivide.call -> fallback(fn(e) -> "error: " + e.message) }
            test { self(10, 2) == "result: 5" }
        }
        "#,
        r#"
            console.log(compute(10, 2));
        "#,
    ), "result: 5");
}

// ─── Method chaining ────────────────────────────────────

#[test]
fn method_chaining_on_string() {
    assert_eq!(run(
        r#"
        pub fn process(input: String) -> String {
            const trimmed = input.trim()
            const upper = trimmed.toUpperCase()
            return upper
            crash {
                input.trim -> skip
                trimmed.toUpperCase -> skip
            }
            test { self(" hello ") == "HELLO" }
        }
        "#,
        r#"console.log(process("  hello  "));"#,
    ), "HELLO");
}

#[test]
fn string_length() {
    assert_eq!(run(
        r#"
        pub fn char_count(s: String) -> Number {
            return s.length
            test { self("hello") == 5 }
        }
        "#,
        r#"console.log(char_count("hello world"));"#,
    ), "11");
}

// ─── Complex: multiple contracts on same struct ─────────

#[test]
fn struct_satisfies_three_contracts() {
    assert_eq!(run(
        r#"
        contract Stringable { to_string() -> String }
        contract Measurable { size() -> Number }
        contract Describable { describe() -> String }

        pub struct Config {
            name: String
            value: Number
        }{}

        Config satisfies Stringable {
            fn to_string() -> String {
                return self.name + "=" + self.value
                test { self() == "timeout=30" }
            }
        }

        Config satisfies Measurable {
            fn size() -> Number {
                return self.value
                test { self() == 30 }
            }
        }

        Config satisfies Describable {
            fn describe() -> String {
                return "Config(" + self.name + ")"
                test { self() == "Config(timeout)" }
            }
        }
        "#,
        r#"
            const c = new Config({ name: "timeout", value: 30 });
            console.log(c.to_string());
            console.log(c.size());
            console.log(c.describe());
        "#,
    ), "timeout=30\n30\nConfig(timeout)");
}

// ─── Error propagation patterns ─────────────────────────

#[test]
fn error_checked_with_truthiness() {
    // In JS, Error objects are truthy, null is falsy
    assert_eq!(run(
        r#"
        /// Validates input
        pub struct Validate {
            call(s: String) -> String, err {
                err empty = "empty"
                err invalid = "invalid"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.empty }
                if s == "bad" { return err.invalid }
                return s
                test {
                    self("ok") == "ok"
                    self("") is err.empty
                    self("bad") is err.invalid
                }
            }
        }
        "#,
        r#"
            const { value: v1, err: e1 } = Validate.call("hello");
            console.log(e1 ? "error" : "ok");

            const { value: v2, err: e2 } = Validate.call("");
            console.log(e2 ? "error" : "ok");
            console.log(e2.message);

            const { value: v3, err: e3 } = Validate.call("bad");
            console.log(e3 ? "error" : "ok");
            console.log(e3.message);
        "#,
    ), "ok\nerror\nempty\nerror\ninvalid");
}

// ─── Enum contract used as value ────────────────────────

#[test]
fn enum_contract_used_in_logic() {
    assert_eq!(run(
        r#"
        contract StatusCode { 200 400 500 }

        pub fn status_message(code: Number) -> String {
            if code == 200 { return "ok" }
            if code == 400 { return "bad request" }
            if code == 500 { return "server error" }
            return "unknown"
            test {
                self(200) == "ok"
                self(400) == "bad request"
                self(999) == "unknown"
            }
        }
        "#,
        r#"
            console.log(StatusCode["200"]);
            console.log(status_message(200));
            console.log(status_message(400));
            console.log(status_message(500));
            console.log(status_message(999));
        "#,
    ), "200\nok\nbad request\nserver error\nunknown");
}
