use super::harness::run;

#[test]
fn err_variable_message_access() {
    // err.message on a variable from let x, err = ... should work
    assert_eq!(run(
        r#"pub fn check(s: String) -> String, err {
            if s == "" { return err.empty }
            return s
            test { self("ok") == "ok" self("") is err.empty }
        }
        pub fn caller(s: String) -> String {
            let result, err = check(s)
            if err { return "error: " + err.message }
            return result
            crash { check -> halt }
            test { self("ok") == "ok" }
        }"#,
        r#"console.log(caller("ok")); console.log(caller(""));"#,
    ), "ok\nerror: empty");
}

#[test]
fn success_returns_value_and_null() {
    assert_eq!(run(
        r#"pub fn divide(a: Number, b: Number) -> Number, err {
            if b == 0 { return err.division_by_zero }
            return a / b
            test { self(10, 2) == 5 self(10, 0) is err.division_by_zero }
        }"#,
        r#"
            const { value: val, err } = divide(10, 2);
            console.log(val);
            console.log(err);
        "#,
    ), "5\nnull");
}

#[test]
fn error_returns_zero_value_and_error() {
    // Go-style: error return includes zero value of the type, not null
    assert_eq!(run(
        r#"pub fn divide(a: Number, b: Number) -> Number, err {
            if b == 0 { return err.division_by_zero }
            return a / b
            test { self(10, 2) == 5 self(10, 0) is err.division_by_zero }
        }"#,
        r#"
            const { value: val, err } = divide(10, 0);
            console.log(val);
            console.log(typeof val);
            console.log(err.name);
            console.log(err.message);
        "#,
    ), "0\nnumber\ndivision_by_zero\ndivision_by_zero");
}

#[test]
fn multiple_error_paths() {
    assert_eq!(run(
        r#"pub fn parse_age(s: String) -> Number, err {
            if s == "" { return err.empty }
            if s == "bad" { return err.invalid }
            return 25
            test {
                self("ok") == 25
                self("") is err.empty
                self("bad") is err.invalid
            }
        }"#,
        r#"
            const { value: v1, err: e1 } = parse_age("ok");
            console.log(v1);
            const { value: v2, err: e2 } = parse_age("");
            console.log(e2.message);
            const { value: v3, err: e3 } = parse_age("bad");
            console.log(e3.message);
        "#,
    ), "25\nempty\ninvalid");
}

#[test]
fn err_tuple_destructure_in_caller() {
    assert_eq!(run(
        r#"pub fn validate(s: String) -> String, err {
            if s == "" { return err.empty }
            return s
            test { self("a") == "a" self("") is err.empty }
        }
        pub fn process(s: String) -> String {
            const result = validate(s)
            return "ok"
            crash { validate -> halt }
            test { self("a") == "ok" }
        }"#,
        r#"
            const { value: val, err } = validate("hello");
            console.log(val);
            console.log(err);
        "#,
    ), "hello\nnull");
}

#[test]
fn non_err_function_returns_plain_value() {
    // Functions without ,err should return plain values, not tuples
    assert_eq!(run(
        r#"pub fn add(a: Number, b: Number) -> Number {
            return a + b
            test { self(1, 2) == 3 }
        }"#,
        r#"
            const result = add(1, 2);
            console.log(result);
            console.log(typeof result);
        "#,
    ), "3\nnumber");
}
