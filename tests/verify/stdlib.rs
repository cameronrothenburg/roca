use super::harness::run;

// ─── log() maps to console.log() ───────────────────────

#[test]
fn log_emits_console_log() {
    assert_eq!(run(
        r#"pub fn greet(name: String) -> String {
            log("hello " + name)
            return name
            crash { log -> skip }
            test { self("cam") == "cam" }
        }"#,
        r#"greet("world");"#,
    ), "hello world");
}

#[test]
fn log_multiple_calls() {
    assert_eq!(run(
        r#"pub fn count() -> Number {
            log("one")
            log("two")
            log("three")
            return 3
            crash {
                log -> skip
            }
            test { self() == 3 }
        }"#,
        "count();",
    ), "one\ntwo\nthree");
}

// ─── Map type ───────────────────────────────────────────

#[test]
fn map_basic_operations() {
    // Map() is a constructor call — emitter maps uppercase calls to new X()
    assert_eq!(run(
        r#"pub fn use_map() -> Bool {
            const m = Map()
            m.set("name", "cam")
            m.set("city", "rothenburg")
            const val = m.get("name")
            return m.has("name")
            crash {
                Map -> skip
                m.set -> skip
                m.get -> skip
                m.has -> skip
            }
            test { self() == true }
        }"#,
        r#"
            console.log(use_map());
            // Verify actual get value from JS
            const m = new Map();
            m.set("name", "cam");
            console.log(m.get("name"));
        "#,
    ), "true\ncam");
}

#[test]
fn map_has_and_size() {
    assert_eq!(run(
        r#"pub fn check_map() -> Bool {
            const m = Map()
            m.set("key", "val")
            return m.has("key")
            crash {
                Map -> skip
                m.set -> skip
                m.has -> skip
            }
            test { self() == true }
        }"#,
        "console.log(check_map());",
    ), "true");
}

// ─── Method checking catches bad calls ──────────────────

#[test]
fn string_valid_methods_compile() {
    // This should compile and run — all methods exist on String contract
    assert_eq!(run(
        r#"pub fn process(s: String) -> String {
            const trimmed = s.trim()
            const upper = trimmed.toUpperCase()
            const has_a = upper.includes("A")
            return upper
            crash {
                s.trim -> skip
                trimmed.toUpperCase -> skip
                upper.includes -> skip
            }
            test { self(" hello ") == "HELLO" }
        }"#,
        r#"console.log(process(" hello "));"#,
    ), "HELLO");
}

#[test]
fn number_tostring_works() {
    assert_eq!(run(
        r#"pub fn show(n: Number) -> String {
            return n.toString()
            crash { n.toString -> skip }
            test { self(42) == "42" }
        }"#,
        "console.log(show(42));",
    ), "42");
}

#[test]
fn number_tofixed_works() {
    assert_eq!(run(
        r#"pub fn format(n: Number) -> String {
            return n.toFixed(2)
            crash { n.toFixed -> skip }
            test { self(3.14159) == "3.14" }
        }"#,
        "console.log(format(3.14159));",
    ), "3.14");
}
