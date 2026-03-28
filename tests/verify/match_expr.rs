use super::harness::run;

#[test]
fn match_number() {
    assert_eq!(run(
        r#"pub fn describe(code: Number) -> String {
            return match code {
                200 => "ok"
                404 => "not found"
                500 => "error"
                _ => "unknown"
            }
            test {
                self(200) == "ok"
                self(404) == "not found"
                self(999) == "unknown"
            }
        }"#,
        r#"
            console.log(describe(200));
            console.log(describe(404));
            console.log(describe(500));
            console.log(describe(999));
        "#,
    ), "ok\nnot found\nerror\nunknown");
}

#[test]
fn match_string() {
    assert_eq!(run(
        r#"pub fn greet(lang: String) -> String {
            return match lang {
                "en" => "hello"
                "es" => "hola"
                "de" => "hallo"
                _ => "hi"
            }
            test {
                self("en") == "hello"
                self("fr") == "hi"
            }
        }"#,
        r#"
            console.log(greet("en"));
            console.log(greet("es"));
            console.log(greet("fr"));
        "#,
    ), "hello\nhola\nhi");
}

#[test]
fn match_in_variable() {
    assert_eq!(run(
        r#"pub fn label(x: Number) -> String {
            const msg = match x {
                1 => "one"
                2 => "two"
                _ => "other"
            }
            return msg
            test {
                self(1) == "one"
                self(2) == "two"
                self(99) == "other"
            }
        }"#,
        "console.log(label(1)); console.log(label(2)); console.log(label(99));",
    ), "one\ntwo\nother");
}

#[test]
fn match_no_default() {
    assert_eq!(run(
        r#"pub fn check(x: Number) -> String {
            return match x {
                0 => "zero"
                1 => "one"
            }
            test { self(0) == "zero" }
        }"#,
        "console.log(check(0)); console.log(check(1));",
    ), "zero\none");
}

#[test]
fn match_bool() {
    assert_eq!(run(
        r#"pub fn yesno(b: Bool) -> String {
            return match b {
                true => "yes"
                false => "no"
            }
            test {
                self(true) == "yes"
                self(false) == "no"
            }
        }"#,
        "console.log(yesno(true)); console.log(yesno(false));",
    ), "yes\nno");
}
