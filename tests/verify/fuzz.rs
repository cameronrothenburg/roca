use super::harness::{run, run_with_tests};

#[test]
fn fuzz_tests_generated_for_pub_functions() {
    // Function with String param gets fuzz cases (empty, whitespace, long, etc.)
    let file = roca::parse::parse(r#"
        pub fn greet(name: String) -> String {
            return "Hello " + name
            test { self("cam") == "Hello cam" }
        }
    "#);
    let result = roca::emit::test_harness::emit_tests(&file, "__embed__");
    let (js, count) = result.unwrap();
    // 1 explicit test + 6 fuzz cases for String param
    assert!(count > 1, "expected fuzz tests generated, got count={}", count);
    assert!(js.contains("fuzz"), "test harness should contain fuzz labels");
}

#[test]
fn fuzz_tests_for_number_param() {
    let file = roca::parse::parse(r#"
        pub fn double(n: Number) -> Number {
            return n * 2
            test { self(5) == 10 }
        }
    "#);
    let result = roca::emit::test_harness::emit_tests(&file, "__embed__");
    let (_, count) = result.unwrap();
    // 1 explicit + 5 number fuzz cases (0, -1, MAX, MIN, 0.3)
    assert!(count > 1, "expected fuzz tests, got count={}", count);
}

#[test]
fn fuzz_tests_for_two_params() {
    let file = roca::parse::parse(r#"
        pub fn add(a: Number, b: Number) -> Number {
            return a + b
            test { self(1, 2) == 3 }
        }
    "#);
    let result = roca::emit::test_harness::emit_tests(&file, "__embed__");
    let (_, count) = result.unwrap();
    // 1 explicit + up to 10 combo fuzz cases
    assert!(count > 1, "expected fuzz tests, got count={}", count);
}

#[test]
fn fuzz_doesnt_crash_valid_function() {
    // Function handles all edge cases — fuzz should all pass
    let result = run_with_tests(
        r#"
        pub fn safe(s: String) -> String {
            if s == "" { return "empty" }
            return s
            test { self("hello") == "hello" }
        }
        "#,
        "",
    );
    // Should have "N passed, 0 failed" — all fuzz cases pass
    assert!(result.contains("0 failed"), "fuzz should not fail on safe function, got: {}", result);
}

#[test]
fn fuzz_no_tests_for_private_functions() {
    let file = roca::parse::parse(r#"
        fn helper(s: String) -> String {
            return s
            test { self("a") == "a" }
        }
    "#);
    let result = roca::emit::test_harness::emit_tests(&file, "__embed__");
    let (_, count) = result.unwrap();
    // Only 1 explicit test — no fuzz for private functions
    assert_eq!(count, 1, "private functions should not get fuzz tests");
}

#[test]
fn fuzz_no_tests_for_no_params() {
    let file = roca::parse::parse(r#"
        pub fn hello() -> String {
            return "hello"
            test { self() == "hello" }
        }
    "#);
    let result = roca::emit::test_harness::emit_tests(&file, "__embed__");
    let (_, count) = result.unwrap();
    // Only 1 explicit — no fuzz when no params
    assert_eq!(count, 1, "no-param functions should not get fuzz tests");
}
