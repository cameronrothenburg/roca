use super::harness::run;

#[test]
fn returns_number() {
    assert_eq!(run(
        r#"pub fn add(a: Number, b: Number) -> Number {
            return a + b
            test { self(1, 2) == 3 }
        }"#,
        "console.log(add(1, 2));",
    ), "3");
}

#[test]
fn returns_string() {
    assert_eq!(run(
        r#"pub fn greet(name: String) -> String {
            return "Hello " + name
            test { self("cam") == "Hello cam" }
        }"#,
        r#"console.log(greet("world"));"#,
    ), "Hello world");
}

#[test]
fn returns_bool() {
    assert_eq!(run(
        r#"pub fn is_positive(n: Number) -> Bool {
            if n > 0 { return true }
            return false
            test { self(1) == true self(0) == false }
        }"#,
        "console.log(is_positive(5)); console.log(is_positive(-3));",
    ), "true\nfalse");
}

#[test]
fn no_params() {
    assert_eq!(run(
        r#"pub fn hello() -> String {
            return "hello"
            test { self() == "hello" }
        }"#,
        "console.log(hello());",
    ), "hello");
}

#[test]
fn multiple_functions() {
    assert_eq!(run(
        r#"pub fn double(x: Number) -> Number {
            return x * 2
            test { self(5) == 10 }
        }
        pub fn add_one(x: Number) -> Number {
            return x + 1
            test { self(5) == 6 }
        }"#,
        "console.log(add_one(double(5)));",
    ), "11");
}

#[test]
fn private_not_exported() {
    assert_eq!(run(
        r#"fn helper(x: Number) -> Number {
            return x + 1
            test { self(0) == 1 }
        }
        pub fn use_helper(x: Number) -> Number {
            return helper(x) + helper(x)
            crash { helper -> halt }
            test { self(5) == 12 }
        }"#,
        "console.log(use_helper(5));",
    ), "12");
}

#[test]
fn arithmetic_operators() {
    assert_eq!(run(
        r#"pub fn math(a: Number, b: Number) -> Number {
            return (a + b) * (a - b) / b
            test { self(10, 5) == 15 }
        }"#,
        "console.log(math(10, 5));",
    ), "15");
}

#[test]
fn string_concat_multiple() {
    assert_eq!(run(
        r#"pub fn full_name(first: String, last: String) -> String {
            return first + " " + last
            test { self("John", "Doe") == "John Doe" }
        }"#,
        r#"console.log(full_name("John", "Doe"));"#,
    ), "John Doe");
}
