use super::harness::run;

// ─── Nested match with error arms inside if ────────────

#[test]
fn nested_match_err_in_if() {
    assert_eq!(run(
        r#"pub fn check(flag: Bool, code: Number) -> String, err {
            if flag {
                return match code {
                    200 => "ok"
                    _ => err.bad
                }
            }
            return "skipped"
            test {
                self(true, 200) == "ok"
                self(true, 500) is err.bad
                self(false, 200) == "skipped"
            }
        }"#,
        r#"
            const { value: r1, err: e1 } = check(true, 200);
            console.log(r1);
            const { value: r2, err: e2 } = check(true, 500);
            console.log(e2.message);
            const { value: r3, err: e3 } = check(false, 200);
            console.log(r3);
        "#,
    ), "ok\nbad\nskipped");
}

// ─── self.field assign in satisfies method ─────────────

#[test]
fn self_field_in_satisfies() {
    assert_eq!(run(
        r#"contract Settable { set_name(n: String) -> String }

        pub struct User {
            name: String
        }{}

        User satisfies Settable {
            fn set_name(n: String) -> String {
                self.name = n
                return self.name
                test { self("cam") == "cam" }
            }
        }"#,
        r#"
            const u = new User({ name: "anon" });
            console.log(u.set_name("cam"));
            console.log(u.name);
        "#,
    ), "cam\ncam");
}

// ─── Multiple wait calls ──────────────────────────────

#[test]
fn multiple_sequential_waits() {
    // Verify two sequential waits emit two awaits and the function is async
    let file = roca::parse::parse(r#"
        pub fn fetch_both(api: String) -> String {
            let a, f1 = wait api.getA("/a")
            if f1 { return "fail1" }
            let b, f2 = wait api.getB("/b")
            if f2 { return "fail2" }
            return a + ":" + b
            crash {
                api.getA -> halt
                api.getB -> halt
            }
            test { self("x") == "x:x" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("async function"), "should be async");
    let await_count = js.matches("await").count();
    assert!(await_count >= 2, "should have at least 2 awaits, got {}", await_count);
    // Verify both variables are declared
    assert!(js.contains("let a"), "should declare a");
    assert!(js.contains("let b"), "should declare b");
    assert!(js.contains("let f1"), "should declare f1");
    assert!(js.contains("let f2"), "should declare f2");
}

// ─── Closure captures outer variable ──────────────────

#[test]
fn closure_captures_outer() {
    assert_eq!(run(
        r#"pub fn prefix_all() -> String {
            const prefix = "hi-"
            const items = ["a", "b", "c"]
            const result = items.map(fn(x) -> prefix + x)
            return result.join(",")
            crash {
                items.map -> skip
                result.join -> skip
            }
            test { self() == "hi-a,hi-b,hi-c" }
        }"#,
        "console.log(prefix_all());",
    ), "hi-a,hi-b,hi-c");
}

// ─── String interpolation with method call ────────────

#[test]
fn string_interp_with_method() {
    assert_eq!(run(
        r#"pub fn clean(name: String) -> String {
            return "name: {name.trim()}"
            crash { name.trim -> skip }
            test { self("  cam  ") == "name: cam" }
        }"#,
        r#"console.log(clean("  hello  "));"#,
    ), "name: hello");
}

// ─── For loop over array with push ────────────────────

#[test]
fn for_loop_with_push() {
    assert_eq!(run(
        r#"pub fn double_list() -> String {
            const items = [1, 2, 3]
            let result = []
            for item in items {
                result.push(item * 2)
            }
            return result.join(",")
            crash {
                result.push -> skip
                result.join -> skip
            }
            test { self() == "2,4,6" }
        }"#,
        "console.log(double_list());",
    ), "2,4,6");
}

// ─── Match with all error arms ────────────────────────

#[test]
fn match_all_error_arms() {
    assert_eq!(run(
        r#"pub fn fail_all(x: Number) -> String, err {
            return match x {
                1 => err.a
                2 => err.b
                _ => err.c
            }
            test {
                self(1) is err.a
                self(2) is err.b
                self(99) is err.c
            }
        }"#,
        r#"
            const { value: r1, err: e1 } = fail_all(1);
            console.log(e1.message);
            const { value: r2, err: e2 } = fail_all(2);
            console.log(e2.message);
            const { value: r3, err: e3 } = fail_all(99);
            console.log(e3.message);
        "#,
    ), "a\nb\nc");
}

// ─── Nested if/else with returns ──────────────────────

#[test]
fn deeply_nested_if_else() {
    assert_eq!(run(
        r#"pub fn classify(x: Number) -> String {
            if x > 100 {
                if x > 1000 {
                    return "huge"
                } else {
                    return "big"
                }
            } else {
                if x > 0 {
                    if x > 50 {
                        return "medium"
                    } else {
                        return "small"
                    }
                } else {
                    return "zero-or-neg"
                }
            }
            test {
                self(2000) == "huge"
                self(200) == "big"
                self(75) == "medium"
                self(10) == "small"
                self(-5) == "zero-or-neg"
            }
        }"#,
        r#"
            console.log(classify(2000));
            console.log(classify(200));
            console.log(classify(75));
            console.log(classify(10));
            console.log(classify(-5));
        "#,
    ), "huge\nbig\nmedium\nsmall\nzero-or-neg");
}
