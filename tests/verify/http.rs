
#[test]
fn http_contract_registered() {
    let file = roca::parse::parse(r#"
        import { Http } from std::http
        /// Fetches a URL and returns the status
        pub fn get_status(url: String) -> Number {
            const resp = wait Http.get(url)
            return resp.status()
            crash { Http.get -> fallback(fn(e) -> Http) }
            test { self("https://example.com") == 200 }
        }
    "#);
    let errors = roca::check::check(&file);
    let real: Vec<_> = errors.iter()
        .filter(|e| e.code != "missing-doc" && e.code != "missing-test")
        .collect();
    assert!(real.is_empty(), "unexpected errors: {:?}", real);
}

#[test]
fn http_post_registered() {
    let file = roca::parse::parse(r#"
        import { Http } from std::http
        /// Posts data and returns status
        pub fn send(url: String, data: String) -> Number {
            const resp = wait Http.post(url, data)
            return resp.status()
            crash { Http.post -> fallback(fn(e) -> Http) }
            test { self("https://example.com", "body") == 201 }
        }
    "#);
    let errors = roca::check::check(&file);
    let real: Vec<_> = errors.iter()
        .filter(|e| e.code != "missing-doc" && e.code != "missing-test")
        .collect();
    assert!(real.is_empty(), "unexpected errors: {:?}", real);
}

#[test]
fn http_js_uses_runtime() {
    let file = roca::parse::parse(r#"
        import { Http } from std::http
        /// Simple fetch
        pub fn get_body(url: String) -> String {
            const resp = wait Http.get(url)
            const body = wait resp.text()
            return body
            crash {
                Http.get -> fallback(fn(e) -> Http)
                resp.text -> fallback(fn(e) -> "error")
            }
            test { self("https://example.com") == "mock body" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("import roca from \"@rocalang/runtime\""), "should import runtime: {}", js);
    assert!(js.contains("roca.Http"), "should use roca.Http: {}", js);
}
