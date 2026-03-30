use super::harness::run;

#[test]
fn time_now_returns_positive() {
    let result = run(
        r#"
        import { Time } from std::time
        pub fn ts() -> Number {
            return Time.now()
            test { self() == self() }
        }
        "#,
        r#"
            const v = ts();
            console.log(v > 0 ? "ok" : "fail");
        "#,
    );
    assert_eq!(result, "ok");
}

#[test]
fn time_parse_valid_iso() {
    assert_eq!(run(
        r#"
        import { Time } from std::time
        pub fn parse_ts(s: String) -> Number {
            const ts = Time.parse(s)
            return ts
            crash { Time.parse -> halt }
            test { self("2026-01-01T00:00:00Z") == self("2026-01-01T00:00:00Z") }
        }
        "#,
        r#"
            const v = parse_ts("2026-01-01T00:00:00Z");
            console.log(v > 0 ? "ok" : "fail");
        "#,
    ), "ok");
}

#[test]
fn time_parse_invalid() {
    assert_eq!(run(
        r#"
        import { Time } from std::time
        pub fn try_parse(s: String) -> Number {
            const ts = Time.parse(s)
            return ts
            crash { Time.parse -> fallback(fn(e) -> 0) }
            test { self("2026-01-01") == self("2026-01-01") }
        }
        "#,
        r#"
            const v = try_parse("not a date");
            console.log(v === 0 ? "fallback" : "parsed");
        "#,
    ), "fallback");
}
