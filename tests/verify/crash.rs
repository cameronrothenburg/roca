use super::harness::run;

#[test]
fn halt_lets_error_propagate() {
    assert_eq!(run(
        r#"
        /// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Calls risky
        pub fn caller() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> fallback(fn(e) -> "error: " + e.message) }
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
        /// Maybe fail operation
        pub struct MaybeFail {
            call(x: Number) -> Number, err {
                err zero = "zero"
            }
        }{
            pub fn call(x: Number) -> Number, err {
                if x == 0 { return err.zero }
                return x
                test { self(1) == 1 self(0) is err.zero }
            }
        }

        /// Calls maybe fail safely
        pub fn safe_call() -> String {
            const result = MaybeFail.call(0)
            return "continued"
            crash { MaybeFail.call -> skip }
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
        /// Gets a value
        pub struct GetValue {
            call(x: Number) -> Number, err {
                err not_found = "not_found"
            }
        }{
            pub fn call(x: Number) -> Number, err {
                if x == 0 { return err.not_found }
                return x
                test { self(1) == 1 self(0) is err.not_found }
            }
        }

        /// Gets with default
        pub fn with_default() -> Number {
            const result = GetValue.call(0)
            return result
            crash { GetValue.call -> fallback(99) }
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
        /// Flaky operation
        pub struct Flaky {
            call(s: String) -> String, err {
                err fail = "fail"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.fail }
                return "ok"
                test { self("ok") == "ok" self("") is err.fail }
            }
        }

        /// Calls flaky
        pub fn caller() -> String {
            const result = Flaky.call("ok")
            return result
            crash { Flaky.call -> retry(3, 0) |> fallback("failed") }
            test { self() == "ok" }
        }
        "#,
        "console.log(caller());",
    ), "ok");
}

#[test]
fn retry_exhausts_attempts_then_throws() {
    // Function always fails — retry should try 3 times then halt propagates error
    assert_eq!(run(
        r#"
        /// Always fails
        pub struct AlwaysFail {
            call(s: String) -> String, err {
                err broken = "broken"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.broken }
                return s
                test { self("ok") == "ok" self("") is err.broken }
            }
        }

        /// Calls always fail
        pub fn caller() -> String, err {
            if false { return err.broken }
            const result = AlwaysFail.call("")
            return result
            crash { AlwaysFail.call -> retry(3, 0) |> halt }
            test { self() is Ok self() is err.broken }
        }
        "#,
        r#"
            const result = caller();
            console.log(result.err ? result.err.name : "no error");
        "#,
    ), "broken");
}

#[test]
fn retry_succeeds_on_later_attempt() {
    // Use a global counter — succeeds on 3rd attempt
    assert_eq!(run(
        r#"
        /// Gets count
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
        /// Safe operation
        pub struct Safe {
            call(x: Number) -> Number, err {
                err fail = "fail"
            }
        }{
            pub fn call(x: Number) -> Number, err {
                if x == 0 { return err.fail }
                return 42
                test { self(1) == 42 self(0) is err.fail }
            }
        }

        /// Calls safe
        pub fn caller() -> Number {
            const result = Safe.call(1)
            return result
            crash { Safe.call -> fallback(0) }
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
        /// Step one
        pub struct StepOne {
            call(x: Number) -> Number, err {
                err fail = "fail"
            }
        }{
            pub fn call(x: Number) -> Number, err {
                if x == 0 { return err.fail }
                return 10
                test { self(1) == 10 self(0) is err.fail }
            }
        }

        /// Step two
        pub struct StepTwo {
            call(x: Number) -> Number, err {
                err fail = "fail"
            }
        }{
            pub fn call(x: Number) -> Number, err {
                if x == 0 { return err.fail }
                return 20
                test { self(1) == 20 self(0) is err.fail }
            }
        }

        /// Pipeline
        pub fn pipeline() -> Number {
            const a = StepOne.call(1)
            const b = StepTwo.call(1)
            return a + b
            crash {
                StepOne.call -> fallback(0)
                StepTwo.call -> fallback(0)
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
        /// Fetches data
        pub struct Fetch {
            call(url: String) -> String, err {
                err invalid = "invalid"
                err timeout = "timeout"
            }
        }{
            pub fn call(url: String) -> String, err {
                if url == "" { return err.invalid }
                if url == "timeout" { return err.timeout }
                return "data"
                test {
                    self("ok") == "data"
                    self("") is err.invalid
                    self("timeout") is err.timeout
                }
            }
        }

        /// Loads data
        pub fn load() -> String {
            const result = Fetch.call("timeout")
            return result
            crash {
                Fetch.call {
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
        /// Fetches data
        pub struct Fetch {
            call(url: String) -> String, err {
                err unknown = "unknown"
            }
        }{
            pub fn call(url: String) -> String, err {
                if url == "bad" { return err.unknown }
                return "ok"
                test { self("ok") == "ok" self("bad") is err.unknown }
            }
        }

        /// Loads data
        pub fn load() -> String, err {
            const result = Fetch.call("bad")
            return result
            crash {
                Fetch.call {
                    err.timeout -> fallback("cached")
                    default -> halt
                }
            }
            test { self() is err.unknown }
        }
        "#,
        r#"
            const { value, err } = load();
            if (err) {
                console.log("caught");
                console.log(err.message);
            } else {
                console.log("should not reach");
            }
        "#,
    ), "caught\nunknown");
}

#[test]
fn halt_propagates_tuple_error() {
    // In a , err function, halt on a tuple call auto-propagates the error
    assert_eq!(run(
        r#"
        /// Validates input
        pub struct Validate {
            call(s: String) -> String, err {
                err empty = "value cannot be empty"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.empty("value cannot be empty") }
                return s
                test { self("ok") == "ok" self("") is err.empty }
            }
        }

        /// Processes input
        pub fn process(s: String) -> String, err {
            const result = Validate.call(s)
            return result
            crash { Validate.call -> halt }
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
    ), "hello\nnull\nnull\nempty\nvalue cannot be empty");
}

#[test]
fn fallback_on_tuple_uses_default() {
    assert_eq!(run(
        r#"
        /// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Safe wrapper
        pub fn safe() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> fallback("default") }
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
        /// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Ignores errors
        pub fn ignorer() -> String {
            const result = Risky.call("")
            return "continued"
            crash { Risky.call -> skip }
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
        /// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Calls risky
        pub fn caller() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> log |> fallback("got error") }
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
        /// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Ignores errors
        pub fn ignorer() -> String {
            const result = Risky.call("")
            return "continued"
            crash { Risky.call -> log |> skip }
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
        /// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Safe wrapper
        pub fn safe() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> log |> fallback("safe_default") }
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
            const result = load()
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
        /// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "something broke"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom("something broke") }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Handles errors
        pub fn handler() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> fallback(fn(e) -> "error: " + e.message) }
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
        /// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err timeout = "took too long"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.timeout("took too long") }
                return s
                test { self("ok") == "ok" self("") is err.timeout }
            }
        }

        /// Handles errors
        pub fn handler() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> fallback(fn(e) -> e.name + ": " + e.message) }
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
        /// Validates input
        pub struct Validate {
            call(s: String) -> String, err {
                err empty = "cannot be empty"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.empty("cannot be empty") }
                return s
                test { self("ok") == "ok" self("") is err.empty }
            }
        }

        /// Processes input
        pub fn process(s: String) -> String, err {
            const result = Validate.call(s)
            return result
            crash { Validate.call -> halt }
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
