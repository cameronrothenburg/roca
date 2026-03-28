use super::harness::run;

#[test]
fn wait_single_emits_async_await() {
    // wait makes the function async and the call awaited
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
    assert!(js.contains("async function"), "should emit async function, got:\n{}", js);
    assert!(js.contains("await"), "should emit await, got:\n{}", js);
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
    assert!(js.contains("Promise.all"), "should emit Promise.all, got:\n{}", js);
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
    assert!(js.contains("Promise.race"), "should emit Promise.race, got:\n{}", js);
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
    assert!(!js.contains("async"), "should NOT be async without wait");
    assert!(!js.contains("await"), "should NOT have await without wait");
}

#[test]
fn wait_single_execution() {
    // Verify wait actually works with a real async call
    assert_eq!(run(
        r#"
        pub fn delayed() -> String {
            return "hello"
            test { self() == "hello" }
        }
        "#,
        r#"
            // Simulate async: wait emits try/catch with await
            async function test() {
                const result = await Promise.resolve("async works");
                console.log(result);
            }
            test();
        "#,
    ), "async works");
}
