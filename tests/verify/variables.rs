use super::harness::run;

#[test]
fn const_binding() {
    assert_eq!(run(
        r#"pub fn msg() -> String {
            const greeting = "hello"
            return greeting
            test { self() == "hello" }
        }"#,
        "console.log(msg());",
    ), "hello");
}

#[test]
fn let_binding() {
    assert_eq!(run(
        r#"pub fn count() -> Number {
            let x = 0
            x = x + 1
            x = x + 1
            x = x + 1
            return x
            test { self() == 3 }
        }"#,
        "console.log(count());",
    ), "3");
}

#[test]
fn multiple_bindings() {
    assert_eq!(run(
        r#"pub fn compute() -> Number {
            const a = 10
            const b = 20
            let result = a + b
            result = result * 2
            return result
            test { self() == 60 }
        }"#,
        "console.log(compute());",
    ), "60");
}

#[test]
fn let_with_string() {
    assert_eq!(run(
        r#"pub fn build() -> String {
            let msg = "hello"
            msg = msg + " world"
            return msg
            test { self() == "hello world" }
        }"#,
        "console.log(build());",
    ), "hello world");
}
