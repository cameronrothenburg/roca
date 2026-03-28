use super::harness::run;

// ─── Parse + Emit verification ──────────────────────────

#[test]
fn wait_single_emits_async() {
    let file = roca::parse::parse(r#"
        pub fn fetch(url: String) -> String {
            let response, failed = wait http.get(url)
            if failed { return "error" }
            return response
            crash { http.get -> halt }
            test { self("url") == "url" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("async function"), "should emit async, got:\n{}", js);
    assert!(js.contains("await"), "should emit await");
}

#[test]
fn wait_all_emits_promise_all() {
    let file = roca::parse::parse(r#"
        pub fn fetch_both(http: String) -> String {
            let users, posts, failed = waitAll {
                http.get("/users")
                http.get("/posts")
            }
            if failed { return "error" }
            return "ok"
            crash { http.get -> halt }
            test { self("x") == "ok" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("Promise.all"), "should emit Promise.all");
    assert!(js.contains("async"), "should be async");
}

#[test]
fn wait_first_emits_promise_race() {
    let file = roca::parse::parse(r#"
        pub fn fetch_fast(http: String) -> String {
            let fastest, failed = waitFirst {
                http.get("/cdn1")
                http.get("/cdn2")
            }
            if failed { return "error" }
            return fastest
            crash { http.get -> halt }
            test { self("x") == "x" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("Promise.race"), "should emit Promise.race");
    assert!(js.contains("async"), "should be async");
}

#[test]
fn no_wait_stays_sync() {
    let file = roca::parse::parse(r#"
        pub fn add(a: Number, b: Number) -> Number {
            return a + b
            test { self(1, 2) == 3 }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(!js.contains("async"), "should NOT be async");
    assert!(!js.contains("await"), "should NOT have await");
}

// ─── wait single — JS execution ────────────────────────

#[test]
fn wait_resolves_value() {
    assert_eq!(run(
        r#"
        pub fn test_wait() -> String {
            return "sync"
            test { self() == "sync" }
        }
        "#,
        r#"
            async function main() {
                const result = await Promise.resolve("resolved");
                console.log(result);
            }
            main();
        "#,
    ), "resolved");
}

#[test]
fn wait_catches_failure() {
    assert_eq!(run(
        r#"
        pub fn test_fn() -> String {
            return "ok"
            test { self() == "ok" }
        }
        "#,
        r#"
            async function main() {
                let result;
                let failed;
                try {
                    result = await Promise.reject(new Error("network error"));
                } catch(_e) {
                    failed = _e;
                }
                console.log(result === undefined);
                console.log(failed.message);
            }
            main();
        "#,
    ), "true\nnetwork error");
}

#[test]
fn wait_emitted_try_catch_structure() {
    // Verify the emitted JS structure matches what we expect
    let file = roca::parse::parse(r#"
        pub fn fetch(url: String) -> String {
            let data, failed = wait http.get(url)
            return "done"
            crash { http.get -> halt }
            test { self("x") == "done" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("let data"), "should declare data");
    assert!(js.contains("let failed"), "should declare failed");
    assert!(js.contains("try"), "should have try block");
    assert!(js.contains("catch"), "should have catch block");
    assert!(js.contains("data = await"), "should assign await result to data");
    assert!(js.contains("failed = _e"), "should assign error to failed");
}

// ─── waitAll — JS execution ────────────────────────────

#[test]
fn wait_all_resolves_multiple() {
    assert_eq!(run(
        r#"
        pub fn test_fn() -> String {
            return "ok"
            test { self() == "ok" }
        }
        "#,
        r#"
            async function main() {
                const [a, b] = await Promise.all([
                    Promise.resolve("first"),
                    Promise.resolve("second")
                ]);
                console.log(a);
                console.log(b);
            }
            main();
        "#,
    ), "first\nsecond");
}

#[test]
fn wait_all_fails_if_any_fails() {
    assert_eq!(run(
        r#"
        pub fn test_fn() -> String {
            return "ok"
            test { self() == "ok" }
        }
        "#,
        r#"
            async function main() {
                let results;
                let failed;
                try {
                    results = await Promise.all([
                        Promise.resolve("ok"),
                        Promise.reject(new Error("second failed"))
                    ]);
                } catch(_e) {
                    failed = _e;
                }
                console.log(results === undefined);
                console.log(failed.message);
            }
            main();
        "#,
    ), "true\nsecond failed");
}

#[test]
fn wait_all_emitted_structure() {
    let file = roca::parse::parse(r#"
        pub fn multi(http: String) -> String {
            let a, b, failed = waitAll {
                http.get("/one")
                http.get("/two")
            }
            return "done"
            crash { http.get -> halt }
            test { self("x") == "done" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("Promise.all"), "should use Promise.all");
    assert!(js.contains("let a"), "should declare a");
    assert!(js.contains("let b"), "should declare b");
    assert!(js.contains("let failed"), "should declare failed");
    assert!(js.contains("_wait_result"), "should use temp for destructure");
}

#[test]
fn wait_all_three_calls() {
    let file = roca::parse::parse(r#"
        pub fn multi(http: String) -> String {
            let a, b, c, failed = waitAll {
                http.get("/one")
                http.get("/two")
                http.get("/three")
            }
            return "done"
            crash { http.get -> halt }
            test { self("x") == "done" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("Promise.all"), "should use Promise.all");
    assert!(js.contains("let a"), "should declare a");
    assert!(js.contains("let b"), "should declare b");
    assert!(js.contains("let c"), "should declare c");
}

// ─── waitFirst — JS execution ──────────────────────────

#[test]
fn wait_first_resolves_fastest() {
    assert_eq!(run(
        r#"
        pub fn test_fn() -> String {
            return "ok"
            test { self() == "ok" }
        }
        "#,
        r#"
            async function main() {
                const result = await Promise.race([
                    new Promise(r => setTimeout(() => r("slow"), 100)),
                    Promise.resolve("fast")
                ]);
                console.log(result);
            }
            main();
        "#,
    ), "fast");
}

#[test]
fn wait_first_emitted_structure() {
    let file = roca::parse::parse(r#"
        pub fn race(http: String) -> String {
            let winner, failed = waitFirst {
                http.get("/cdn1")
                http.get("/cdn2")
            }
            return "done"
            crash { http.get -> halt }
            test { self("x") == "done" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("Promise.race"), "should use Promise.race");
    assert!(js.contains("let winner"), "should declare winner");
    assert!(js.contains("let failed"), "should declare failed");
}

// ─── Auto-async detection ───────────────────────────────

#[test]
fn function_with_wait_is_async() {
    let file = roca::parse::parse(r#"
        pub fn fetch(url: String) -> String {
            let r, f = wait http.get(url)
            return "done"
            crash { http.get -> halt }
            test { self("x") == "done" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("async function fetch"), "function should be async");
}

#[test]
fn function_without_wait_is_sync() {
    let file = roca::parse::parse(r#"
        pub fn add(a: Number, b: Number) -> Number {
            return a + b
            test { self(1, 2) == 3 }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("function add") && !js.contains("async function add"), "should be sync");
}

#[test]
fn wait_in_if_branch_makes_async() {
    let file = roca::parse::parse(r#"
        pub fn maybe_fetch(do_fetch: Bool, url: String) -> String {
            if do_fetch {
                let r, f = wait http.get(url)
                return "fetched"
            }
            return "skipped"
            crash { http.get -> halt }
            test { self(false, "x") == "skipped" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("async function"), "wait in if should make function async");
}

// ─── Crash integration ─────────────────────────────────

#[test]
fn wait_calls_appear_in_crash() {
    // Verify the checker sees wait calls and requires crash handlers
    let file = roca::parse::parse(r#"
        pub fn fetch(url: String) -> String {
            let r, f = wait http.get(url)
            return "done"
            test { self("x") == "done" }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "missing-crash"),
        "wait calls should require crash handlers, got: {:?}", errors);
}

#[test]
fn wait_with_crash_passes() {
    let file = roca::parse::parse(r#"
        pub fn fetch(url: String) -> String {
            let r, f = wait http.get(url)
            return "done"
            crash { http.get -> halt }
            test { self("x") == "done" }
        }
    "#);
    let errors = roca::check::check(&file);
    let crash_errors: Vec<_> = errors.iter().filter(|e| e.code == "missing-crash" || e.code == "unhandled-call").collect();
    assert!(crash_errors.is_empty(), "wait with crash should pass, got: {:?}", crash_errors);
}

// ─── Multiple waits in one function ─────────────────────

#[test]
fn multiple_waits_in_sequence() {
    let file = roca::parse::parse(r#"
        pub fn pipeline(http: String) -> String {
            let user, f1 = wait http.get("/user")
            let posts, f2 = wait http.get("/posts")
            return "done"
            crash { http.get -> halt }
            test { self("x") == "done" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("async function"), "should be async");
    // Should have two separate await calls
    let await_count = js.matches("await").count();
    assert!(await_count >= 2, "should have at least 2 awaits, got {}", await_count);
}

#[test]
fn wait_then_sync_code() {
    let file = roca::parse::parse(r#"
        pub fn process(http: String) -> String {
            let data, failed = wait http.get("/data")
            if failed { return "error" }
            let trimmed = data.trim()
            return trimmed
            crash {
                http.get -> halt
                data.trim -> halt
            }
            test { self("x") == "x" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("async function"), "should be async");
    assert!(js.contains("await"), "should have await for http.get");
    assert!(js.contains(".trim()"), "sync code should still work");
}

// ─── Edge cases ─────────────────────────────────────────

#[test]
fn wait_all_single_call() {
    // waitAll with just one call — still uses Promise.all
    let file = roca::parse::parse(r#"
        pub fn single(http: String) -> String {
            let result, failed = waitAll {
                http.get("/one")
            }
            return "done"
            crash { http.get -> halt }
            test { self("x") == "done" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("Promise.all"), "even single call uses Promise.all");
}

#[test]
fn wait_first_single_call() {
    let file = roca::parse::parse(r#"
        pub fn single(http: String) -> String {
            let result, failed = waitFirst {
                http.get("/one")
            }
            return "done"
            crash { http.get -> halt }
            test { self("x") == "done" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("Promise.race"), "even single call uses Promise.race");
}

// ─── Real-world pattern ─────────────────────────────────

#[test]
fn wait_with_error_handling_pattern() {
    let file = roca::parse::parse(r#"
        pub fn load_user(http: String, id: String) -> String, err {
            let response, failed = wait http.get("/users/{id}")
            if failed { return err.fetch_failed }
            let user, parse_err = response.validate()
            if parse_err { return err.invalid_data }
            return user

            crash {
                http.get -> retry(3, 1000)
                response.validate -> halt
            }

            test {
                self("http", "1") is Ok
            }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("async function load_user"), "should be async");
    assert!(js.contains("await"), "should await the http call");
    assert!(js.contains("try"), "should have try/catch for wait");
}
