//! Diagnostics provider — runs the Roca checker and converts rule errors into LSP diagnostics.

use crate::errors::RuleError;
use tower_lsp::lsp_types::*;

pub fn check_source(source: &str) -> Vec<Diagnostic> {
    let file = match super::safe_parse(source) {
        Some(f) => f,
        None => {
            return vec![Diagnostic {
                range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: "Parse error — syntax may be incomplete or invalid".into(),
                ..Default::default()
            }];
        }
    };
    let errors = crate::check::check(&file);

    errors.iter().map(|e| rule_error_to_diagnostic(e, source)).collect()
}

fn rule_error_to_diagnostic(err: &RuleError, source: &str) -> Diagnostic {
    let range = find_error_range(err, source);

    let mut message = err.message.clone();
    if let Some(ctx) = &err.context {
        message = format!("{}\n  → {}", message, ctx);
    }

    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String(err.code.clone())),
        source: Some("roca".into()),
        message,
        ..Default::default()
    }
}

fn find_error_range(err: &RuleError, source: &str) -> Range {
    let search_term = extract_search_term(err);

    if let Some(term) = search_term {
        for (line_num, line) in source.lines().enumerate() {
            if let Some(col) = line.find(&term) {
                return Range::new(
                    Position::new(line_num as u32, col as u32),
                    Position::new(line_num as u32, (col + term.len()) as u32),
                );
            }
        }
    }

    Range::new(Position::new(0, 0), Position::new(0, 0))
}

fn extract_search_term(err: &RuleError) -> Option<String> {
    let quoted_name = || err.message.split('\'').nth(1).map(str::to_string);

    match err.code.as_str() {
        "missing-test" | "missing-crash" => {
            quoted_name().map(|n| format!("fn {}", n))
        }
        "missing-impl" | "undeclared-method" => {
            err.context.as_ref().and_then(|ctx| ctx.split('.').last().map(str::to_string))
        }
        "unknown-contract" | "missing-satisfies" | "satisfies-mismatch" => {
            quoted_name().map(|n| format!("satisfies {}", n))
        }
        "duplicate-err" => {
            quoted_name().map(|n| format!("err {}", n))
        }
        "const-reassign" | "unknown-method" | "not-loggable" | "unhandled-call" | "nullable-access" => {
            quoted_name()
        }
        "untested-error" | "no-success-test" => {
            quoted_name()
        }
        "type-mismatch" | "struct-comparison" | "invalid-ordering" => {
            quoted_name()
        }
        "invalid-constraint" => {
            quoted_name()
        }
        "no-fn-error-def" => {
            // Message: "standalone function 'name' cannot define error 'err_name'"
            // Search for the return err.name statement
            err.message.split('\'').nth(3).map(|n| format!("return err.{}", n))
        }
        "unhandled-error" => {
            // Message: "error 'name' propagates via halt in 'fn_name' but is not declared"
            quoted_name().map(|n| format!("err.{}", n))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_code(diags: &[Diagnostic], code: &str) -> bool {
        diags.iter().any(|d| {
            matches!(&d.code, Some(NumberOrString::String(c)) if c == code)
        })
    }

    fn get_with_code<'a>(diags: &'a [Diagnostic], code: &str) -> Option<&'a Diagnostic> {
        diags.iter().find(|d| {
            matches!(&d.code, Some(NumberOrString::String(c)) if c == code)
        })
    }

    // ─── Valid code produces zero diagnostics ───────────

    #[test]
    fn valid_function_no_diagnostics() {
        let diags = check_source(r#"
            /// Adds two numbers
            pub fn add(a: Number, b: Number) -> Number {
                return a + b
                test { self(1, 2) == 3 }
            }
        "#);
        assert!(diags.is_empty(), "expected no diagnostics, got: {:?}", diags);
    }

    #[test]
    fn valid_struct_no_diagnostics() {
        let diags = check_source(r#"
            /// An email address
            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err invalid = "bad"
                }
            }{
                fn validate(raw: String) -> Email, err {
                    if raw == "" { return err.invalid }
                    return Email { value: raw }
                    test { self("a@b") is Ok self("") is err.invalid }
                }
            }
        "#);
        assert!(diags.is_empty(), "expected no diagnostics, got: {:?}", diags);
    }

    #[test]
    fn valid_full_program_no_diagnostics() {
        let diags = check_source(r#"
            contract Stringable { to_string() -> String }

            /// A person's name
            pub struct Name { value: String }{}

            Name satisfies Stringable {
                fn to_string() -> String {
                    return self.value
                    test {}
                }
            }

            /// Greets a person by name
            pub fn greet(name: String) -> String {
                let trimmed = name.trim()
                return "Hello " + trimmed
                test { self("cam") == "Hello cam" }
            }
        "#);
        assert!(diags.is_empty(), "expected no diagnostics, got: {:?}", diags);
    }

    // ─── missing-test ───────────────────────────────────

    #[test]
    fn missing_test_detected() {
        let diags = check_source(r#"
            pub fn add(a: Number, b: Number) -> Number {
                return a + b
            }
        "#);
        assert!(has_code(&diags, "missing-test"));
    }

    #[test]
    fn missing_test_positioned_at_fn() {
        let diags = check_source("pub fn greet(name: String) -> String {\n    return name\n}");
        let d = get_with_code(&diags, "missing-test").unwrap();
        // Should point to "fn greet"
        assert!(d.range.start.line == 0, "expected line 0, got {}", d.range.start.line);
    }

    // ─── missing-crash ──────────────────────────────────

    #[test]
    fn missing_crash_detected() {
        let diags = check_source(r#"
            pub fn validate(s: String) -> String, err {
                err empty = "empty"
                if s == "" { return err.empty }
                return s
                test { self("a") == "a" self("") is err.empty }
            }
            pub fn caller() -> String, err {
                err empty = "empty"
                const r = validate("x")
                return r
                test { self() == "x" }
            }
        "#);
        assert!(has_code(&diags, "missing-crash"));
    }

    // ─── const-reassign ─────────────────────────────────

    #[test]
    fn const_reassign_detected() {
        let diags = check_source(r#"
            pub fn bad() -> Number {
                const x = 5
                x = 10
                return x
                test { self() == 10 }
            }
        "#);
        assert!(has_code(&diags, "const-reassign"));
    }

    #[test]
    fn const_reassign_positioned_at_name() {
        let src = "pub fn bad() -> Number {\n    const x = 5\n    x = 10\n    return x\n    test { self() == 10 }\n}";
        let diags = check_source(src);
        let d = get_with_code(&diags, "const-reassign").unwrap();
        // "x" appears on line 1 (const x) — the search finds the first occurrence
        assert!(d.range.start.character > 0, "should point to 'x' not start of line");
    }

    // ─── missing-impl ───────────────────────────────────

    #[test]
    fn missing_impl_detected() {
        let diags = check_source(r#"
            pub struct Email {
                value: String
                validate(raw: String) -> Email
                to_string() -> String
            }{
                fn validate(raw: String) -> Email {
                    return Email { value: raw }
                    test {}
                }
            }
        "#);
        assert!(has_code(&diags, "missing-impl"));
        let d = get_with_code(&diags, "missing-impl").unwrap();
        assert!(d.message.contains("to_string"));
    }

    // ─── unknown-contract ───────────────────────────────

    #[test]
    fn unknown_contract_detected() {
        let diags = check_source(r#"
            pub struct Email { value: String }{}
            Email satisfies DoesNotExist {
                fn foo() -> String {
                    return "bar"
                    test {}
                }
            }
        "#);
        assert!(has_code(&diags, "unknown-contract"));
        let d = get_with_code(&diags, "unknown-contract").unwrap();
        assert!(d.message.contains("DoesNotExist"));
    }

    // ─── missing-satisfies ──────────────────────────────

    #[test]
    fn missing_satisfies_method_detected() {
        let diags = check_source(r#"
            contract Serializable {
                serialize() -> String
                deserialize(raw: String) -> String
            }
            pub struct Name { value: String }{}
            Name satisfies Serializable {
                fn serialize() -> String {
                    return self.value
                    test {}
                }
            }
        "#);
        assert!(has_code(&diags, "missing-satisfies"));
        let d = get_with_code(&diags, "missing-satisfies").unwrap();
        assert!(d.message.contains("deserialize"));
    }

    // ─── duplicate-err ──────────────────────────────────

    #[test]
    fn duplicate_err_detected() {
        let diags = check_source(r#"
            contract Bad {
                get(url: String) -> String, err {
                    err timeout = "a"
                    err timeout = "b"
                }
            }
        "#);
        assert!(has_code(&diags, "duplicate-err"));
    }

    // ─── unknown-method ─────────────────────────────────

    #[test]
    fn unknown_method_detected() {
        let diags = check_source(r#"
            pub fn bad(s: String) -> String {
                return s.fakefn()
                crash { s.fakefn -> halt }
                test { self("a") == "a" }
            }
        "#);
        assert!(has_code(&diags, "unknown-method"));
        let d = get_with_code(&diags, "unknown-method").unwrap();
        assert!(d.message.contains("String"));
        assert!(d.message.contains("fakefn"));
    }

    #[test]
    fn unknown_method_shows_available() {
        let diags = check_source(r#"
            pub fn bad(n: Number) -> String {
                return n.trim()
                crash { n.trim -> halt }
                test { self(1) == "1" }
            }
        "#);
        let d = get_with_code(&diags, "unknown-method").unwrap();
        assert!(d.message.contains("available:"), "should show available methods");
    }

    // ─── not-loggable ───────────────────────────────────

    #[test]
    fn not_loggable_detected() {
        let diags = check_source(r#"
            pub fn bad() -> String {
                const arr = ["a"]
                log(arr)
                return "done"
                crash { log -> halt }
                test { self() == "done" }
            }
        "#);
        assert!(has_code(&diags, "not-loggable"));
        let d = get_with_code(&diags, "not-loggable").unwrap();
        assert!(d.message.contains("to_log"));
    }

    // ─── unhandled-call ─────────────────────────────────

    #[test]
    fn unhandled_call_detected() {
        let diags = check_source(r#"
            pub fn a() -> String, err {
                err fail = "fail"
                return "ok"
                test { self() == "ok" }
            }
            pub fn b() -> String, err {
                err fail = "fail"
                return "ok"
                test { self() == "ok" }
            }
            pub fn caller() -> String, err {
                err fail = "fail"
                const x = a()
                const y = b()
                return x
                crash { a -> halt }
                test { self() == "ok" }
            }
        "#);
        assert!(has_code(&diags, "unhandled-call"));
        let d = get_with_code(&diags, "unhandled-call").unwrap();
        assert!(d.message.contains("b"));
    }

    // ─── Parse error handling ───────────────────────────

    #[test]
    fn parse_error_doesnt_crash() {
        let diags = check_source("pub fn {{{{{ broken garbage @#$%");
        assert!(!diags.is_empty(), "should produce at least one diagnostic");
        assert!(diags[0].message.contains("Parse error") || diags[0].message.contains("parse"));
    }

    #[test]
    fn incomplete_code_doesnt_crash() {
        let diags = check_source("pub fn greet(name: String) -> String {");
        assert!(!diags.is_empty());
    }

    #[test]
    fn empty_file_no_diagnostics() {
        let diags = check_source("");
        assert!(diags.is_empty());
    }

    #[test]
    fn comment_only_no_diagnostics() {
        let diags = check_source("// this is a comment\n// another one");
        assert!(diags.is_empty());
    }

    // ─── All diagnostics have correct fields ────────────

    #[test]
    fn diagnostics_have_source() {
        let diags = check_source("pub fn bad() -> Number { return 1 }");
        for d in &diags {
            assert_eq!(d.source.as_deref(), Some("roca"));
        }
    }

    #[test]
    fn diagnostics_have_error_severity() {
        let diags = check_source("pub fn bad() -> Number { return 1 }");
        for d in &diags {
            assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        }
    }

    #[test]
    fn diagnostics_have_error_code() {
        let diags = check_source("pub fn bad() -> Number { return 1 }");
        for d in &diags {
            assert!(d.code.is_some(), "diagnostic should have an error code");
        }
    }

    // ─── Multiple errors in one file ────────────────────

    #[test]
    fn multiple_errors_all_reported() {
        let diags = check_source(r#"
            pub fn one() -> Number { return 1 }
            pub fn two() -> Number { return 2 }
        "#);
        // Both functions missing test blocks
        let test_errors: Vec<_> = diags.iter()
            .filter(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "missing-test"))
            .collect();
        assert_eq!(test_errors.len(), 2, "should report missing-test for both functions");
    }

    #[test]
    fn mixed_errors_all_reported() {
        let diags = check_source(r#"
            pub fn bad() -> Number {
                const x = 5
                x = 10
                let y = x.fakefn()
                return 0
            }
        "#);
        // missing-test + const-reassign + missing-crash + unknown-method
        assert!(diags.len() >= 3, "expected multiple errors, got {}", diags.len());
    }

    // ─── Test coverage errors ───────────────────────────

    #[test]
    fn untested_error_detected() {
        let diags = check_source(r#"
            pub fn validate(s: String) -> String, err {
                if s == "" { return err.empty }
                if s == "bad" { return err.invalid }
                return s
                test {
                    self("ok") == "ok"
                    self("") is err.empty
                }
            }
        "#);
        assert!(has_code(&diags, "untested-error"),
            "should detect untested 'invalid' error");
        let d = get_with_code(&diags, "untested-error").unwrap();
        assert!(d.message.contains("invalid"));
    }

    #[test]
    fn no_success_test_detected() {
        let diags = check_source(r#"
            pub fn validate(s: String) -> String, err {
                if s == "" { return err.empty }
                return s
                test {
                    self("") is err.empty
                }
            }
        "#);
        assert!(has_code(&diags, "no-success-test"),
            "should require a success test case");
    }

    #[test]
    fn full_coverage_no_errors() {
        let diags = check_source(r#"
            pub fn validate(s: String) -> String, err {
                if s == "" { return err.empty }
                if s == "bad" { return err.invalid }
                return s
                test {
                    self("ok") == "ok"
                    self("") is err.empty
                    self("bad") is err.invalid
                }
            }
        "#);
        let coverage_errors: Vec<_> = diags.iter()
            .filter(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "untested-error" || c == "no-success-test"))
            .collect();
        assert!(coverage_errors.is_empty(), "full coverage should pass, got: {:?}", coverage_errors);
    }
}
