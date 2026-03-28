use super::harness::run;

// ─── String() conversion ────────────────────────────────

#[test]
fn number_to_string() {
    assert_eq!(run(
        r#"pub fn convert(n: Number) -> String {
            return String(n)
            crash { String -> halt }
            test { self(42) == "42" }
        }"#,
        "console.log(convert(42));",
    ), "42");
}

#[test]
fn bool_to_string_via_convert() {
    assert_eq!(run(
        r#"pub fn convert(b: Bool) -> String {
            return String(b)
            crash { String -> halt }
            test { self(true) == "true" }
        }"#,
        "console.log(convert(true)); console.log(convert(false));",
    ), "true\nfalse");
}

#[test]
fn string_to_string_noop() {
    assert_eq!(run(
        r#"pub fn convert(s: String) -> String {
            return String(s)
            crash { String -> halt }
            test { self("hello") == "hello" }
        }"#,
        r#"console.log(convert("hello"));"#,
    ), "hello");
}

// ─── Number() conversion ───────────────────────────────

#[test]
fn string_to_number() {
    assert_eq!(run(
        r#"pub fn convert(s: String) -> Number {
            return Number(s)
            crash { Number -> halt }
            test { self("42") == 42 }
        }"#,
        r#"console.log(convert("42")); console.log(convert("3.14"));"#,
    ), "42\n3.14");
}

#[test]
fn bool_to_number() {
    assert_eq!(run(
        r#"pub fn convert(b: Bool) -> Number {
            return Number(b)
            crash { Number -> halt }
            test { self(true) == 1 }
        }"#,
        "console.log(convert(true)); console.log(convert(false));",
    ), "1\n0");
}

// ─── Bool() conversion ─────────────────────────────────

#[test]
fn string_to_bool() {
    assert_eq!(run(
        r#"pub fn convert(s: String) -> Bool {
            return Bool(s)
            crash { Bool -> halt }
            test { self("hello") == true }
        }"#,
        r#"console.log(convert("hello")); console.log(convert(""));"#,
    ), "true\nfalse");
}

#[test]
fn number_to_bool() {
    assert_eq!(run(
        r#"pub fn convert(n: Number) -> Bool {
            return Bool(n)
            crash { Bool -> halt }
            test { self(1) == true }
        }"#,
        "console.log(convert(1)); console.log(convert(0));",
    ), "true\nfalse");
}

// ─── String concat patterns ────────────────────────────

#[test]
fn concat_with_plus() {
    assert_eq!(run(
        r#"pub fn greet(first: String, last: String) -> String {
            return first + " " + last
            test { self("John", "Doe") == "John Doe" }
        }"#,
        r#"console.log(greet("Jane", "Smith"));"#,
    ), "Jane Smith");
}

#[test]
fn concat_number_needs_conversion() {
    assert_eq!(run(
        r#"pub fn label(name: String, age: Number) -> String {
            return name + " is " + String(age)
            crash { String -> halt }
            test { self("cam", 25) == "cam is 25" }
        }"#,
        r#"console.log(label("cam", 25));"#,
    ), "cam is 25");
}

#[test]
fn interpolation_still_works() {
    assert_eq!(run(
        r#"pub fn greet(name: String) -> String {
            return "hello {name}!"
            test { self("world") == "hello world!" }
        }"#,
        r#"console.log(greet("roca"));"#,
    ), "hello roca!");
}

// ─── Struct constructor still uses new ──────────────────

#[test]
fn struct_still_uses_new() {
    let file = roca::parse::parse(r#"
        pub struct Email { value: String }{}
        pub fn make() -> String {
            const e = Email { value: "test" }
            return "ok"
            test { self() == "ok" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("new Email"), "struct should use new, got:\n{}", js);
}

#[test]
fn string_conversion_no_new() {
    let file = roca::parse::parse(r#"
        pub fn convert(n: Number) -> String {
            return String(n)
            crash { String -> halt }
            test { self(42) == "42" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(!js.contains("new String"), "String() should NOT use new, got:\n{}", js);
    assert!(js.contains("String(n)"), "should emit String(n)");
}
