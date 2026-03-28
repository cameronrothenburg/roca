use super::harness::{run, run_expect_fail};

// ─── What CAN be logged (has to_log) ────────────────────

#[test]
fn log_string_works() {
    // String has to_log() in stdlib
    assert_eq!(run(
        r#"pub fn greet(name: String) -> String {
            log(name)
            return name
            crash { log -> halt }
            test { self("cam") == "cam" }
        }"#,
        r#"greet("cam");"#,
    ), "cam");
}

#[test]
fn log_number_works() {
    // Number has to_log() in stdlib
    assert_eq!(run(
        r#"pub fn show(n: Number) -> Number {
            log(n)
            return n
            crash { log -> halt }
            test { self(42) == 42 }
        }"#,
        "show(42);",
    ), "42");
}

#[test]
fn log_bool_works() {
    // Bool has to_log() in stdlib
    assert_eq!(run(
        r#"pub fn check(b: Bool) -> Bool {
            log(b)
            return b
            crash { log -> halt }
            test { self(true) == true }
        }"#,
        "check(true);",
    ), "true");
}

#[test]
fn error_works_like_log() {
    // error() emits to stderr — verify it compiles and returns correctly
    assert_eq!(run(
        r#"pub fn fail(msg: String) -> String {
            error(msg)
            return msg
            crash { error -> halt }
            test { self("oops") == "oops" }
        }"#,
        r#"console.log(fail("oops"));"#,
    ), "oops");
}

#[test]
fn warn_works_like_log() {
    // warn() emits to stderr — verify it compiles and returns correctly
    assert_eq!(run(
        r#"pub fn caution(msg: String) -> String {
            warn(msg)
            return msg
            crash { warn -> halt }
            test { self("careful") == "careful" }
        }"#,
        r#"console.log(caution("careful"));"#,
    ), "careful");
}

#[test]
fn log_with_to_log_call() {
    // Calling .to_log() explicitly always works
    assert_eq!(run(
        r#"pub fn show(n: Number) -> String {
            const msg = n.toString()
            log(msg)
            return msg
            crash {
                n.toString -> halt
                log -> halt
            }
            test { self(42) == "42" }
        }"#,
        "show(42);",
    ), "42");
}

// ─── What CANNOT be logged (no to_log) ──────────────────

#[test]
fn array_cannot_be_logged() {
    // Array does NOT have to_log() — compiler should reject
    let file = roca::parse::parse(r#"
        pub fn bad() -> String {
            const arr = ["a", "b"]
            log(arr)
            return "done"
            crash { log -> halt }
            test { self() == "done" }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "not-loggable"),
        "expected not-loggable error, got: {:?}", errors);
}

#[test]
fn map_cannot_be_logged() {
    // Map does NOT have to_log() — compiler should reject
    let file = roca::parse::parse(r#"
        pub fn bad() -> String {
            const m = Map()
            log(m)
            return "done"
            crash {
                Map -> halt
                log -> halt
            }
            test { self() == "done" }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "not-loggable"),
        "expected not-loggable error, got: {:?}", errors);
}

#[test]
fn custom_struct_cannot_be_logged_without_to_log() {
    // Custom struct without to_log() — compiler should reject
    let file = roca::parse::parse(r#"
        pub struct Secret {
            value: String
        }{}

        pub fn bad(s: Secret) -> String {
            log(s)
            return "done"
            crash { log -> halt }
            test { self(s) == "done" }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "not-loggable"),
        "expected not-loggable error for Secret, got: {:?}", errors);
}

#[test]
fn error_also_requires_loggable() {
    let file = roca::parse::parse(r#"
        pub fn bad() -> String {
            const arr = ["a"]
            error(arr)
            return "done"
            crash { error -> halt }
            test { self() == "done" }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "not-loggable"),
        "expected not-loggable error for error(), got: {:?}", errors);
}

#[test]
fn warn_also_requires_loggable() {
    let file = roca::parse::parse(r#"
        pub fn bad() -> String {
            const arr = ["a"]
            warn(arr)
            return "done"
            crash { warn -> halt }
            test { self() == "done" }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "not-loggable"),
        "expected not-loggable error for warn(), got: {:?}", errors);
}

// ─── PPI protection pattern ─────────────────────────────

#[test]
fn secret_with_to_log_redacts() {
    assert_eq!(run(
        r#"
        contract Loggable { to_log() -> String }

        pub struct Secret {
            value: String
            create(v: String) -> Secret
        }{
            fn create(v: String) -> Secret {
                return Secret { value: v }
                test {}
            }
        }

        Secret satisfies Loggable {
            fn to_log() -> String {
                return "REDACTED"
                test {}
            }
        }

        pub fn process(s: String) -> String {
            const secret = Secret.create(s)
            log(secret.to_log())
            return "done"
            crash {
                Secret.create -> halt
                secret.to_log -> halt
                log -> halt
            }
            test { self("my-password") == "done" }
        }
        "#,
        r#"process("super-secret-password");"#,
    ), "REDACTED");
}

#[test]
fn email_with_to_log_shows_value() {
    assert_eq!(run(
        r#"
        contract Loggable { to_log() -> String }

        pub struct Email {
            value: String
            create(v: String) -> Email
        }{
            fn create(v: String) -> Email {
                return Email { value: v }
                test {}
            }
        }

        Email satisfies Loggable {
            fn to_log() -> String {
                return self.value
                test {}
            }
        }

        pub fn process(email_raw: String) -> String {
            const email = Email.create(email_raw)
            log(email.to_log())
            return "done"
            crash {
                Email.create -> halt
                email.to_log -> halt
                log -> halt
            }
            test { self("cam@test.com") == "done" }
        }
        "#,
        r#"process("cam@test.com");"#,
    ), "cam@test.com");
}

#[test]
fn credit_card_logs_last_four() {
    assert_eq!(run(
        r#"
        contract Loggable { to_log() -> String }

        pub struct CreditCard {
            number: String
            create(n: String) -> CreditCard
        }{
            fn create(n: String) -> CreditCard {
                return CreditCard { number: n }
                test {}
            }
        }

        CreditCard satisfies Loggable {
            fn to_log() -> String {
                return "****" + self.number.slice(12)
                crash { self.number.slice -> halt }
                test {}
            }
        }

        pub fn charge(card_num: String) -> String {
            const card = CreditCard.create(card_num)
            log(card.to_log())
            return "charged"
            crash {
                CreditCard.create -> halt
                card.to_log -> halt
                log -> halt
            }
            test { self("4242424242421234") == "charged" }
        }
        "#,
        r#"charge("4242424242421234");"#,
    ), "****1234");
}
