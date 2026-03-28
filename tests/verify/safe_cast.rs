use super::harness::run;

// ─── Number() safe cast ─────────────────────────────────

#[test]
fn number_cast_valid_string() {
    assert_eq!(run(
        r#"pub fn parse(s: String) -> Number {
            let n, err = Number(s)
            if err { return 0 }
            return n
            crash { Number -> halt }
            test { self("42") == 42 }
        }"#,
        r#"console.log(parse("42")); console.log(parse("3.14"));"#,
    ), "42\n3.14");
}

#[test]
fn number_cast_invalid_string() {
    assert_eq!(run(
        r#"pub fn parse(s: String) -> Number {
            let n, err = Number(s)
            if err { return -1 }
            return n
            crash { Number -> halt }
            test { self("hello") == -1 self("42") == 42 }
        }"#,
        r#"console.log(parse("hello")); console.log(parse("abc"));"#,
    ), "-1\n-1");
}

#[test]
fn number_cast_null() {
    assert_eq!(run(
        r#"pub fn parse_or_zero(val: String) -> Number {
            let n, err = Number(val)
            if err { return 0 }
            return n
            crash { Number -> halt }
            test { self("5") == 5 }
        }"#,
        r#"console.log(parse_or_zero(null));"#,
    ), "0");
}

// ─── String() safe cast ────────────────────────────────

#[test]
fn string_cast_number() {
    assert_eq!(run(
        r#"pub fn to_str(n: Number) -> String {
            let s, err = String(n)
            if err { return "error" }
            return s
            crash { String -> halt }
            test { self(42) == "42" }
        }"#,
        "console.log(to_str(42));",
    ), "42");
}

#[test]
fn string_cast_null() {
    assert_eq!(run(
        r#"pub fn to_str(val: String) -> String {
            let s, err = String(val)
            if err { return "was null" }
            return s
            crash { String -> halt }
            test { self("hello") == "hello" }
        }"#,
        r#"console.log(to_str(null));"#,
    ), "was null");
}

// ─── Bool() safe cast ──────────────────────────────────

#[test]
fn bool_cast_truthy() {
    assert_eq!(run(
        r#"pub fn to_bool(s: String) -> Bool {
            let b, err = Bool(s)
            if err { return false }
            return b
            crash { Bool -> halt }
            test { self("hello") == true }
        }"#,
        r#"console.log(to_bool("hello")); console.log(to_bool(""));"#,
    ), "true\nfalse");
}

#[test]
fn bool_cast_null() {
    assert_eq!(run(
        r#"pub fn to_bool(val: String) -> Bool {
            let b, err = Bool(val)
            if err { return false }
            return b
            crash { Bool -> halt }
            test { self("x") == true }
        }"#,
        "console.log(to_bool(null));",
    ), "false");
}

// ─── null keyword ───────────────────────────────────────

#[test]
fn null_literal() {
    assert_eq!(run(
        r#"pub fn check(s: String) -> Bool {
            if s == null { return true }
            return false
            test { self("hello") == false }
        }"#,
        r#"console.log(check(null)); console.log(check("hello"));"#,
    ), "true\nfalse");
}

#[test]
fn null_assignment() {
    assert_eq!(run(
        r#"pub fn make_null() -> String {
            const x = null
            if x == null { return "is null" }
            return "not null"
            test { self() == "is null" }
        }"#,
        "console.log(make_null());",
    ), "is null");
}

// ─── Error messages ─────────────────────────────────────

#[test]
fn number_cast_error_message() {
    assert_eq!(run(
        r#"pub fn parse(s: String) -> String {
            let n, e = Number(s)
            if e { return e.message }
            return "ok"
            crash { Number -> halt }
            test { self("42") == "ok" }
        }"#,
        r#"console.log(parse("hello"));"#,
    ), "invalid_number");
}

#[test]
fn string_cast_null_error_message() {
    assert_eq!(run(
        r#"pub fn cast(val: String) -> String {
            let s, e = String(val)
            if e { return e.message }
            return s
            crash { String -> halt }
            test { self("hello") == "hello" }
        }"#,
        "console.log(cast(null));",
    ), "invalid_string");
}
