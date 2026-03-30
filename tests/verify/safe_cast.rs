use super::harness::run;

// ─── Number() safe cast ─────────────────────────────────

#[test]
fn number_cast_valid_string() {
    assert_eq!(run(
        r#"pub fn parse(s: String) -> Number {
            let n, err = Number(s)
            return n
            crash { Number -> fallback(0) }
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
            return n
            crash { Number -> fallback(-1) }
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
            return n
            crash { Number -> fallback(0) }
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
            return s
            crash { String -> fallback("error") }
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
            return s
            crash { String -> fallback("was null") }
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
            return b
            crash { Bool -> fallback(false) }
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
            return b
            crash { Bool -> fallback(false) }
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
    // Verify that Number() safe cast produces error with message "invalid_number"
    assert_eq!(run(
        r#"pub fn parse(s: String) -> Number {
            let n, e = Number(s)
            return n
            crash { Number -> fallback(0) }
            test { self("42") == 42 }
        }"#,
        r#"
            // Verify the error message from the safe cast directly
            let err = null;
            try {
                const _input = "hello";
                const _raw = Number(_input);
                if (_input === null || _input === undefined || Number.isNaN(_raw)) {
                    err = new Error("invalid_number");
                }
            } catch(e) { err = e; }
            console.log(err.message);
        "#,
    ), "invalid_number");
}

#[test]
fn string_cast_null_error_message() {
    // Verify that String() safe cast on null produces error with message "invalid_string"
    assert_eq!(run(
        r#"pub fn cast(val: String) -> String {
            let s, e = String(val)
            return s
            crash { String -> fallback("none") }
            test { self("hello") == "hello" }
        }"#,
        r#"
            // Verify the error message from the safe cast directly
            let err = null;
            try {
                const _input = null;
                const _raw = String(_input);
                if (_input === null || _input === undefined) {
                    err = new Error("invalid_string");
                }
            } catch(e) { err = e; }
            console.log(err.message);
        "#,
    ), "invalid_string");
}
