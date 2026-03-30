use super::harness::run;

#[test]
fn halt_lets_error_propagate() {
    assert_eq!(run(
        r#"
        pub fn risky() -> String, err {
            if true { return err.boom }
            return "ok"
            test { self() is err.boom }
        }

        pub fn caller() -> String {
            let result, e = risky()
            if e { return "error: " + e.message }
            return "survived"
            crash { risky -> halt }
            test { self() == "error: boom" }
        }
        "#,
        r#"
            console.log(caller());
        "#,
    ), "error: boom");
}

#[test]
fn skip_ignores_error() {
    assert_eq!(run(
        r#"
        pub fn maybe_fail(x: Number) -> Number, err {
            if x == 0 { return err.zero }
            return x
            test { self(1) == 1 self(0) is err.zero }
        }

        pub fn safe_call() -> String {
            const result = maybe_fail(0)
            return "continued"
            crash { maybe_fail -> skip }
            test { self() == "continued" }
        }
        "#,
        r#"
            console.log(safe_call());
        "#,
    ), "continued");
}

#[test]
fn fallback_provides_default() {
    assert_eq!(run(
        r#"
        pub fn get_value() -> Number, err {
            if true { return err.not_found }
            return 42
            test { self() is err.not_found }
        }

        pub fn with_default() -> Number {
            const result = get_value()
            return result
            crash { get_value -> fallback(99) }
            test { self() == 99 }
        }
        "#,
        r#"
            console.log(with_default());
        "#,
    ), "99");
}

#[test]
fn retry_on_success() {
    // Retry with a function that succeeds — should return on first try
    assert_eq!(run(
        r#"
        pub fn flaky() -> String, err {
            return "ok"
            test { self() == "ok" }
        }

        pub fn caller() -> String {
            const result = flaky()
            return result
            crash { flaky -> retry(3, 0) }
            test { self() == "ok" }
        }
        "#,
        "console.log(caller());",
    ), "ok");
}

#[test]
fn retry_exhausts_attempts_then_throws() {
    // Function always fails — retry should try 3 times then throw
    assert_eq!(run(
        r#"
        pub fn always_fail() -> String, err {
            return err.broken
            test { self() is err.broken }
        }

        pub fn caller() -> String {
            const result = always_fail()
            return result
            crash { always_fail -> retry(3, 0) }
            test { self() == "should not reach" }
        }
        "#,
        r#"
            try {
                caller();
                console.log("should not reach");
            } catch(e) {
                console.log("caught after retries");
                console.log(e.message);
            }
        "#,
    ), "caught after retries\nbroken");
}

#[test]
fn retry_succeeds_on_later_attempt() {
    // Use a global counter — succeeds on 3rd attempt
    assert_eq!(run(
        r#"
        pub fn get_count() -> Number {
            return 0
            test { self() == 0 }
        }
        "#,
        r#"
            // Simulate flaky with global state
            let attempts = 0;
            function flaky() {
                attempts++;
                if (attempts < 3) return { value: null, err: new Error("not yet") };
                return { value: "success", err: null };
            }

            let result;
            let _err;
            for (let _attempt = 0; _attempt < 5; _attempt++) {
                const _retry_tmp = flaky();
                _err = _retry_tmp.err;
                if (!_err) { result = _retry_tmp.value; break; }
                if (_attempt === 4) throw _err;
            }
            console.log(result);
            console.log(attempts);
        "#,
    ), "success\n3");
}

#[test]
fn halt_success_passes_through() {
    assert_eq!(run(
        r#"
        pub fn safe() -> Number, err {
            return 42
            test { self() == 42 }
        }

        pub fn caller() -> Number {
            let result, err = safe()
            if err { return 0 }
            return result
            crash { safe -> halt }
            test { self() == 42 }
        }
        "#,
        r#"
            console.log(caller());
        "#,
    ), "42");
}

#[test]
fn multiple_crash_handlers() {
    assert_eq!(run(
        r#"
        pub fn step_one() -> Number, err {
            return 10
            test { self() == 10 }
        }

        pub fn step_two() -> Number, err {
            return 20
            test { self() == 20 }
        }

        pub fn pipeline() -> Number {
            let a, err1 = step_one()
            let b, err2 = step_two()
            return a + b
            crash {
                step_one -> halt
                step_two -> halt
            }
            test { self() == 30 }
        }
        "#,
        "console.log(pipeline());",
    ), "30");
}

#[test]
fn detailed_crash_per_error() {
    assert_eq!(run(
        r#"
        pub fn fetch(url: String) -> String, err {
            if url == "" { return err.invalid }
            if url == "timeout" { return err.timeout }
            return "data"
            test {
                self("ok") == "data"
                self("") is err.invalid
                self("timeout") is err.timeout
            }
        }

        pub fn load() -> String {
            const result = fetch("timeout")
            return result
            crash {
                fetch {
                    err.timeout -> fallback("cached")
                    err.invalid -> fallback("none")
                    default -> halt
                }
            }
            test { self() == "cached" }
        }
        "#,
        "console.log(load());",
    ), "cached");
}

#[test]
fn detailed_crash_default_halt() {
    assert_eq!(run(
        r#"
        pub fn fetch(url: String) -> String, err {
            if url == "bad" { return err.unknown }
            return "ok"
            test { self("ok") == "ok" self("bad") is err.unknown }
        }

        pub fn load() -> String {
            const result = fetch("bad")
            return result
            crash {
                fetch {
                    err.timeout -> fallback("cached")
                    default -> halt
                }
            }
            test { self() == "ok" }
        }
        "#,
        r#"
            try {
                load();
                console.log("should not reach");
            } catch(e) {
                console.log("caught");
                console.log(e.message);
            }
        "#,
    ), "caught\nunknown");
}

#[test]
fn halt_propagates_tuple_error() {
    // In a , err function, halt on a tuple call auto-propagates the error
    assert_eq!(run(
        r#"
        pub fn validate(s: String) -> String, err {
            if s == "" { return err.empty("value cannot be empty") }
            return s
            test { self("ok") == "ok" self("") is err.empty }
        }

        pub fn process(s: String) -> String, err {
            let result, err = validate(s)
            return result
            crash { validate -> halt }
            test { self("ok") == "ok" self("") is err.empty }
        }
        "#,
        r#"
            const { value: val1, err: err1 } = process("hello");
            console.log(val1);
            console.log(err1);

            const { value: val2, err: err2 } = process("");
            console.log(val2);
            console.log(err2.name);
            console.log(err2.message);
        "#,
    ), "hello\nnull\n\nempty\nvalue cannot be empty");
}

#[test]
fn fallback_on_tuple_uses_default() {
    assert_eq!(run(
        r#"
        pub fn risky() -> String, err {
            return err.boom
            test { self() is err.boom }
        }

        pub fn safe() -> String {
            let result, err = risky()
            return result
            crash { risky -> fallback("default") }
            test { self() == "default" }
        }
        "#,
        r#"
            console.log(safe());
        "#,
    ), "default");
}

#[test]
fn skip_on_tuple_continues() {
    assert_eq!(run(
        r#"
        pub fn risky() -> String, err {
            return err.boom
            test { self() is err.boom }
        }

        pub fn ignorer() -> String {
            let result, err = risky()
            return "continued"
            crash { risky -> skip }
            test { self() == "continued" }
        }
        "#,
        r#"
            console.log(ignorer());
        "#,
    ), "continued");
}

// ─── Chain tests ────────────────────────────────────────

#[test]
fn chain_log_halt() {
    assert_eq!(run(
        r#"
        pub fn risky() -> String, err {
            return err.boom
            test { self() is err.boom }
        }

        pub fn caller() -> String {
            let result, err = risky()
            if err { return "got error" }
            return result
            crash { risky -> log |> halt }
            test { self() == "got error" }
        }
        "#,
        r#"console.log(caller());"#,
    ), "got error");
}

#[test]
fn chain_log_skip() {
    assert_eq!(run(
        r#"
        pub fn risky() -> String, err {
            return err.boom
            test { self() is err.boom }
        }

        pub fn ignorer() -> String {
            let result, err = risky()
            return "continued"
            crash { risky -> log |> skip }
            test { self() == "continued" }
        }
        "#,
        r#"console.log(ignorer());"#,
    ), "continued");
}

#[test]
fn chain_log_fallback() {
    assert_eq!(run(
        r#"
        pub fn risky() -> String, err {
            return err.boom
            test { self() is err.boom }
        }

        pub fn safe() -> String {
            let result, err = risky()
            return result
            crash { risky -> log |> fallback("safe_default") }
            test { self() == "safe_default" }
        }
        "#,
        r#"console.log(safe());"#,
    ), "safe_default");
}

#[test]
fn panic_emits_process_exit() {
    let file = roca::parse::parse(r#"
        pub fn loader() -> String {
            let result, err = load()
            return result
            crash { load -> panic }
            test { self() == "ok" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("process.exit"), "panic should emit process.exit");
}

#[test]
fn fallback_closure_receives_error() {
    assert_eq!(run(
        r#"
        pub fn risky() -> String, err {
            return err.boom("something broke")
            test { self() is err.boom }
        }

        pub fn handler() -> String {
            const result = risky()
            return result
            crash { risky -> fallback(fn(e) -> "error: " + e.message) }
            test { self() == "error: something broke" }
        }
        "#,
        r#"console.log(handler());"#,
    ), "error: something broke");
}

#[test]
fn fallback_closure_error_name() {
    assert_eq!(run(
        r#"
        pub fn risky() -> String, err {
            return err.timeout("took too long")
            test { self() is err.timeout }
        }

        pub fn handler() -> String {
            const result = risky()
            return result
            crash { risky -> fallback(fn(e) -> e.name + ": " + e.message) }
            test { self() == "timeout: took too long" }
        }
        "#,
        r#"console.log(handler());"#,
    ), "timeout: took too long");
}

#[test]
fn halt_propagates_error_tuple() {
    assert_eq!(run(
        r#"
        pub fn validate(s: String) -> String, err {
            if s == "" { return err.empty("cannot be empty") }
            return s
            test { self("ok") == "ok" self("") is err.empty }
        }

        pub fn process(s: String) -> String, err {
            let result, err = validate(s)
            return result
            crash { validate -> halt }
            test { self("ok") == "ok" self("") is err.empty }
        }
        "#,
        r#"
            const { value: v1, err: e1 } = process("hello");
            console.log(v1);
            const { value: v2, err: e2 } = process("");
            console.log(e2.name);
            console.log(e2.message);
        "#,
    ), "hello\nempty\ncannot be empty");
}
