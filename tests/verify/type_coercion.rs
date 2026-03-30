use super::harness::run;

#[test]
fn number_to_string() {
    assert_eq!(run(
        r#"pub fn stringify(n: Number) -> String {
            return n.toString()
            crash { n.toString -> skip }
            test { self(42) == "42" }
        }"#,
        r#"console.log(stringify(42));"#,
    ), "42");
}

#[test]
fn number_in_string_concat() {
    assert_eq!(run(
        r#"pub fn label(name: String, age: Number) -> String {
            return name + " is " + age.toString()
            crash { age.toString -> skip }
            test { self("cam", 25) == "cam is 25" }
        }"#,
        r#"console.log(label("cam", 25));"#,
    ), "cam is 25");
}

#[test]
fn bool_to_string() {
    assert_eq!(run(
        r#"pub fn show(b: Bool) -> String {
            return b.toString()
            crash { b.toString -> skip }
            test { self(true) == "true" }
        }"#,
        r#"console.log(show(true)); console.log(show(false));"#,
    ), "true\nfalse");
}

#[test]
fn number_literal_method() {
    assert_eq!(run(
        r#"pub fn fixed() -> String {
            const x = 42
            return x.toString()
            crash { x.toString -> skip }
            test { self() == "42" }
        }"#,
        "console.log(fixed());",
    ), "42");
}

#[test]
fn array_join() {
    assert_eq!(run(
        r#"pub fn csv(items: String) -> String {
            const arr = ["a", "b", "c"]
            return arr.join(", ")
            crash { arr.join -> skip }
            test { self("x") == "a, b, c" }
        }"#,
        r#"console.log(csv("x"));"#,
    ), "a, b, c");
}

#[test]
fn string_includes() {
    assert_eq!(run(
        r#"pub fn has_at(s: String) -> Bool {
            return s.includes("@")
            crash { s.includes -> skip }
            test { self("a@b") == true self("nope") == false }
        }"#,
        r#"console.log(has_at("cam@test.com")); console.log(has_at("nope"));"#,
    ), "true\nfalse");
}
