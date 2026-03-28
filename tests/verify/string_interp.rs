use super::harness::run;

#[test]
fn basic_interpolation() {
    assert_eq!(run(
        r#"pub fn greet(name: String) -> String {
            return "hello {name}"
            test { self("cam") == "hello cam" }
        }"#,
        r#"console.log(greet("world"));"#,
    ), "hello world");
}

#[test]
fn multiple_interpolations() {
    assert_eq!(run(
        r#"pub fn intro(name: String, age: Number) -> String {
            return "{name} is {age}"
            crash { age.toString -> halt }
            test { self("cam", 25) == "cam is 25" }
        }"#,
        r#"console.log(intro("cam", 25));"#,
    ), "cam is 25");
}

#[test]
fn interpolation_at_start() {
    assert_eq!(run(
        r#"pub fn show(val: String) -> String {
            return "{val}!"
            test { self("hi") == "hi!" }
        }"#,
        r#"console.log(show("hi"));"#,
    ), "hi!");
}

#[test]
fn interpolation_at_end() {
    assert_eq!(run(
        r#"pub fn show(val: String) -> String {
            return "value: {val}"
            test { self("42") == "value: 42" }
        }"#,
        r#"console.log(show("42"));"#,
    ), "value: 42");
}

#[test]
fn no_interpolation_plain_string() {
    assert_eq!(run(
        r#"pub fn plain() -> String {
            return "no interpolation here"
            test { self() == "no interpolation here" }
        }"#,
        "console.log(plain());",
    ), "no interpolation here");
}

#[test]
fn interpolation_with_method_call() {
    assert_eq!(run(
        r#"pub fn show(n: Number) -> String {
            return "value: {n.toString()}"
            crash { n.toString -> halt }
            test { self(42) == "value: 42" }
        }"#,
        r#"console.log(show(42));"#,
    ), "value: 42");
}
