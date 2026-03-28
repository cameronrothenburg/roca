use std::collections::HashSet;
use crate::ast::*;
use crate::errors::RuleError;

pub fn check_tests(file: &SourceFile) -> Vec<RuleError> {
    let mut errors = Vec::new();

    for item in &file.items {
        match item {
            Item::Function(f) => {
                check_fn_has_test(f, None, &mut errors);
                check_test_coverage(f, &f.errors, None, &mut errors);
            }
            Item::Struct(s) => {
                // Collect errors from struct's contract signatures
                for method in &s.methods {
                    check_fn_has_test(method, Some(&s.name), &mut errors);
                    // Find matching signature in struct contract for error declarations
                    let sig_errors = s.signatures.iter()
                        .find(|sig| sig.name == method.name)
                        .map(|sig| &sig.errors)
                        .unwrap_or(&method.errors);
                    check_test_coverage(method, sig_errors, Some(&s.name), &mut errors);
                }
            }
            Item::Satisfies(sat) => {
                for method in &sat.methods {
                    check_fn_has_test(method, Some(&sat.struct_name), &mut errors);
                }
            }
            _ => {}
        }
    }

    errors
}

fn check_fn_has_test(f: &FnDef, parent: Option<&str>, errors: &mut Vec<RuleError>) {
    if f.test.is_none() {
        let context = match parent {
            Some(p) => format!("{}.{}", p, f.name),
            None => f.name.clone(),
        };
        errors.push(RuleError {
            code: "missing-test".into(),
            message: format!("function '{}' has no test block", f.name),
            context: Some(context),
        });
    }
}

fn check_test_coverage(f: &FnDef, declared_errors: &[ErrDecl], parent: Option<&str>, errors: &mut Vec<RuleError>) {
    let test = match &f.test {
        Some(t) => t,
        None => return,
    };

    // Collect all error names — from declarations AND from return err.name in body
    let mut all_error_names: HashSet<String> = declared_errors.iter().map(|e| e.name.clone()).collect();
    collect_returned_errors(&f.body, &mut all_error_names);

    // If no errors at all and function doesn't return err, no coverage needed
    if all_error_names.is_empty() && !f.returns_err {
        return;
    }

    // Empty test blocks are allowed for instance methods (tested via integration)
    if test.cases.is_empty() {
        return;
    }

    let context = match parent {
        Some(p) => format!("{}.{}", p, f.name),
        None => f.name.clone(),
    };

    // Collect which errors are tested and whether there's a success case
    let mut tested_errors: HashSet<String> = HashSet::new();
    let mut has_success = false;

    for case in &test.cases {
        match case {
            TestCase::Equals { .. } => has_success = true,
            TestCase::IsOk { .. } => has_success = true,
            TestCase::IsErr { err_name, .. } => {
                tested_errors.insert(err_name.clone());
            }
            TestCase::StatusMock { .. } => has_success = true,
        }
    }

    // Check: every error must have a test
    for err_name in &all_error_names {
        if !tested_errors.contains(err_name) {
            errors.push(RuleError {
                code: "untested-error".into(),
                message: format!(
                    "error '{}' not tested — add: self(...) is err.{}",
                    err_name, err_name
                ),
                context: Some(context.clone()),
            });
        }
    }

    // Check: must have at least one success case
    if !has_success && !all_error_names.is_empty() {
        errors.push(RuleError {
            code: "no-success-test".into(),
            message: "test block has no success case — add: self(...) is Ok or self(...) == value".into(),
            context: Some(context),
        });
    }
}

/// Collect error names from `return err.name` statements in the body
fn collect_returned_errors(stmts: &[Stmt], errors: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::ReturnErr(name) => { errors.insert(name.clone()); }
            Stmt::If { then_body, else_body, .. } => {
                collect_returned_errors(then_body, errors);
                if let Some(body) = else_body {
                    collect_returned_errors(body, errors);
                }
            }
            Stmt::For { body, .. } | Stmt::While { body, .. } => collect_returned_errors(body, errors),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn function_with_test_passes() {
        let file = parse::parse(r#"
            fn add(a: Number, b: Number) -> Number {
                return a + b
                test { self(1, 2) == 3 }
            }
        "#);
        let errors = check_tests(&file);
        assert!(errors.is_empty());
    }

    #[test]
    fn function_without_test_fails() {
        let file = parse::parse(r#"
            fn add(a: Number, b: Number) -> Number {
                return a + b
            }
        "#);
        let errors = check_tests(&file);
        assert!(errors.iter().any(|e| e.code == "missing-test"));
    }

    #[test]
    fn all_errors_tested_passes() {
        let file = parse::parse(r#"
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
        let errors = check_tests(&file);
        assert!(errors.is_empty(), "should pass with full coverage, got: {:?}", errors);
    }

    #[test]
    fn missing_error_test_fails() {
        let file = parse::parse(r#"
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
        let errors = check_tests(&file);
        assert!(errors.iter().any(|e| e.code == "untested-error"),
            "should catch untested 'invalid' error, got: {:?}", errors);
    }

    #[test]
    fn missing_success_case_fails() {
        let file = parse::parse(r#"
            pub fn validate(s: String) -> String, err {
                if s == "" { return err.empty }
                return s
                test {
                    self("") is err.empty
                }
            }
        "#);
        let errors = check_tests(&file);
        assert!(errors.iter().any(|e| e.code == "no-success-test"),
            "should require a success test case, got: {:?}", errors);
    }

    #[test]
    fn struct_method_errors_checked() {
        let file = parse::parse(r#"
            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err missing = "required"
                    err invalid = "bad"
                }
            }{
                fn validate(raw: String) -> Email, err {
                    if raw == "" { return err.missing }
                    return Email { value: raw }
                    test {
                        self("a@b") is Ok
                        self("") is err.missing
                    }
                }
            }
        "#);
        let errors = check_tests(&file);
        assert!(errors.iter().any(|e| e.code == "untested-error" && e.message.contains("invalid")),
            "should catch untested 'invalid', got: {:?}", errors);
    }

    #[test]
    fn empty_test_block_allowed() {
        // Instance methods use empty test {} — no coverage check
        let file = parse::parse(r#"
            pub struct Email {
                value: String
                display() -> String
            }{
                fn display() -> String {
                    return self.value
                    test {}
                }
            }
        "#);
        let errors = check_tests(&file);
        assert!(errors.is_empty(), "empty test block should be allowed, got: {:?}", errors);
    }

    #[test]
    fn non_err_function_no_coverage_check() {
        // Functions that don't return errors don't need error coverage
        let file = parse::parse(r#"
            fn add(a: Number, b: Number) -> Number {
                return a + b
                test { self(1, 2) == 3 }
            }
        "#);
        let errors = check_tests(&file);
        assert!(errors.is_empty());
    }

    #[test]
    fn is_ok_counts_as_success() {
        let file = parse::parse(r#"
            pub fn validate(s: String) -> String, err {
                if s == "" { return err.empty }
                return s
                test {
                    self("ok") is Ok
                    self("") is err.empty
                }
            }
        "#);
        let errors = check_tests(&file);
        assert!(errors.is_empty(), "is Ok should count as success, got: {:?}", errors);
    }
}
