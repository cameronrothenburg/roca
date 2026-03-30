use super::harness::run;

#[test]
fn match_returns_error() {
    assert_eq!(run(
        r#"pub fn categorize(code: Number) -> String, err {
            return match code {
                200 => "ok"
                404 => err.not_found
                500 => err.server_error
                _ => err.unknown
            }
            test {
                self(200) == "ok"
                self(404) is err.not_found
                self(500) is err.server_error
                self(999) is err.unknown
            }
        }"#,
        r#"
            const { value: r1, err: e1 } = categorize(200);
            console.log(r1);
            const { value: r2, err: e2 } = categorize(404);
            console.log(e2.message);
            const { value: r3, err: e3 } = categorize(500);
            console.log(e3.message);
        "#,
    ), "ok\nnot_found\nserver_error");
}
