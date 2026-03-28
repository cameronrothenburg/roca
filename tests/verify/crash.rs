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
    // Retry with a function that succeeds — should just return normally
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
        r#"
            console.log(caller());
        "#,
    ), "ok");
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
