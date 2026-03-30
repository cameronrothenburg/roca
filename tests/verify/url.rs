use super::harness::run;

#[test]
fn url_parse_valid() {
    assert_eq!(run(
        r#"
        import { Url } from std::url
        pub fn get_host(raw: String) -> String {
            const url = Url.parse(raw)
            return url.hostname()
            crash { Url.parse -> halt }
            test { self("https://example.com/path") == "example.com" }
        }
        "#,
        r#"console.log(get_host("https://example.com:8080/path?q=1"));"#,
    ), "example.com");
}

#[test]
fn url_parse_invalid() {
    assert_eq!(run(
        r#"
        import { Url } from std::url
        pub fn try_host(raw: String) -> String, err {
            const url = Url.parse(raw)
            return url.hostname()
            crash { Url.parse -> halt }
            test { self("https://example.com") == "example.com" }
        }
        "#,
        r#"
            const { err } = try_host("not a url");
            console.log(err ? err.name : "no error");
        "#,
    ), "parse_failed");
}

#[test]
fn url_is_valid() {
    assert_eq!(run(
        r#"
        import { Url } from std::url
        pub fn check(raw: String) -> Bool {
            return Url.isValid(raw)
            test { self("https://example.com") == true }
        }
        "#,
        r#"
            console.log(check("https://example.com"));
            console.log(check("not a url"));
        "#,
    ), "true\nfalse");
}

#[test]
fn url_parts() {
    assert_eq!(run(
        r#"
        import { Url } from std::url
        pub fn parts(raw: String) -> String {
            const url = Url.parse(raw)
            return url.protocol() + " " + url.pathname() + " " + url.search()
            crash { Url.parse -> halt }
            test { self("https://example.com/path?q=1") == self("https://example.com/path?q=1") }
        }
        "#,
        r#"console.log(parts("https://example.com/path?q=1"));"#,
    ), "https: /path ?q=1");
}

#[test]
fn url_get_param() {
    assert_eq!(run(
        r#"
        import { Url } from std::url
        pub fn param(raw: String, name: String) -> String {
            const url = Url.parse(raw)
            const val = url.getParam(name)
            if val == null { return "none" }
            return val
            crash { Url.parse -> halt }
            test { self("https://x.com?a=1", "a") == "1" }
        }
        "#,
        r#"
            console.log(param("https://x.com?foo=bar&baz=42", "foo"));
            console.log(param("https://x.com?foo=bar", "missing"));
        "#,
    ), "bar\nnone");
}
