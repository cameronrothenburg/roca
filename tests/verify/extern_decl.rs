use super::harness::{run, run_with_tests};

#[test]
fn extern_contract_type_checks_fields() {
    // extern contract fields should be accessible on returned values
    assert_eq!(run(
        r#"
        extern contract NativeResponse {
            status: Number
            ok: Bool
        }

        extern fn fetchData(url: String) -> NativeResponse, err {
            err network = "network error"
        }

        pub fn get_status(url: String) -> Number {
            const resp = wait fetchData(url)
            return resp.status
            crash { fetchData -> fallback(fn(e) -> 0) }
            test { self("http://example.com") == 200 }
        }
        "#,
        r#"
            // Wire up the extern — must return {value, err} tuple
            globalThis.fetchData = async (url) => ({ value: { status: 200, ok: true }, err: null });
            const result = await get_status("http://test.com");
            console.log(result);
        "#,
    ), "200");
}

#[test]
fn extern_fn_no_js_output() {
    // extern declarations should produce no JS code
    let file = roca::parse::parse(r#"
        extern contract NativeResponse {
            status: Number
        }

        extern fn fetchData(url: String) -> NativeResponse, err {
            err network = "network error"
        }

        pub fn hello() -> String {
            return "hello"
            test { self() == "hello" }
        }
    "#);
    let js = roca::emit::emit(&file);
    // Should NOT contain NativeResponse or fetchData definitions
    assert!(!js.contains("class NativeResponse"), "extern contract should not emit JS class");
    assert!(!js.contains("function fetchData"), "extern fn should not emit JS function");
    // Should contain the regular function
    assert!(js.contains("function hello"));
}

#[test]
fn extern_contract_method_call() {
    assert_eq!(run(
        r#"
        extern contract NativeHeaders {
            get(name: String) -> String | null
            has(name: String) -> Bool
        }

        extern fn getHeaders() -> NativeHeaders, err {
            err failed = "failed"
        }

        pub fn check_header() -> String, err {
            if false { return err.failed }
            const headers = wait getHeaders()
            const has = headers.has("content-type")
            if has { return "found" }
            return "missing"
            crash {
                getHeaders -> halt
                headers.has -> skip
            }
            test { self() == "found" self() is err.failed }
        }
        "#,
        r#"
            globalThis.getHeaders = async () => ({
                value: {
                    get: (name) => name === "content-type" ? "application/json" : null,
                    has: (name) => name === "content-type",
                },
                err: null,
            });
            const { value: result } = await check_header();
            console.log(result);
        "#,
    ), "found");
}

#[test]
fn extern_fn_mock_emitted() {
    // Extern fn mock should emit globalThis patches in test harness
    let file = roca::parse::parse(r#"
        extern contract NativeResponse {
            status: Number
            body: String
        }

        extern fn globalFetch(url: String) -> NativeResponse, err {
            err network = "network error"
            mock {
                globalFetch -> NativeResponse { status: 200, body: "ok" }
            }
        }

        pub fn fetch_status(url: String) -> Number {
            const resp = wait globalFetch(url)
            return resp.status
            crash { globalFetch -> fallback(fn(e) -> 0) }
            test { self("http://example.com") == 200 }
        }
    "#);
    let (test_js, _) = roca::emit::test_harness::emit_tests(&file, "__embed__").unwrap();
    assert!(test_js.contains("globalThis.globalFetch"), "mock patch should set globalThis.globalFetch");
    assert!(test_js.contains("status: 200"), "mock should include status: 200");
}

#[test]
fn extern_fn_mock_in_proof_tests() {
    // The full flow: extern fn mock is used during proof tests with async await
    let output = run_with_tests(
        r#"
        extern contract NativeResponse {
            status: Number
            body: String
        }

        extern fn globalFetch(url: String) -> NativeResponse, err {
            err network = "network error"
            mock {
                globalFetch -> NativeResponse { status: 200, body: "ok" }
            }
        }

        pub fn fetch_status(url: String) -> Number {
            const resp = wait globalFetch(url)
            return resp.status
            crash { globalFetch -> fallback(fn(e) -> 0) }
            test { self("http://example.com") == 200 }
        }
        "#,
        r#"
            const result = await fetch_status("http://test.com");
            console.log(result);
        "#,
    );
    assert!(output.contains("passed, 0 failed"), "expected all tests to pass, got: {}", output);
    assert!(output.ends_with("200"), "expected 200 at end, got: {}", output);
}

#[test]
fn extern_fn_mock_works_at_runtime() {
    // Extern fn with mock — manually wire mock and verify it works
    assert_eq!(run(
        r#"
        extern contract NativeResponse {
            status: Number
            body: String
        }

        extern fn globalFetch(url: String) -> NativeResponse, err {
            err network = "network error"
            mock {
                globalFetch -> NativeResponse { status: 200, body: "ok" }
            }
        }

        pub fn fetch_status(url: String) -> Number {
            const resp = wait globalFetch(url)
            return resp.status
            crash { globalFetch -> fallback(fn(e) -> 0) }
            test { self("http://example.com") == 200 }
        }
        "#,
        r#"
            // Wire up mock (same as what test harness would do) — must return {value, err} tuple
            globalThis.globalFetch = async (url) => ({ value: { status: 200, body: "ok" }, err: null });
            const result = await fetch_status("http://test.com");
            console.log(result);
        "#,
    ), "200");
}
