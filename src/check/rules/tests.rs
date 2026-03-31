//! Rule: missing-test, untested-error, no-success-test
//! Validates inline test blocks — coverage of error paths and success cases.

use std::collections::HashSet;
use crate::ast::*;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::FnCheckContext;
use crate::check::walker::type_ref_to_name;

#[cfg(test)]
mod tests {
    use crate::check;

    fn errors(src: &str) -> Vec<crate::errors::RuleError> {
        check::check(&crate::parse::parse(src))
    }

    #[test]
    fn with_test_passes() {
        assert!(!errors(r#"fn add(a: Number, b: Number) -> Number { return a + b test { self(1, 2) == 3 } }"#)
            .iter().any(|e| e.code == "missing-test"));
    }

    #[test]
    fn missing_test() {
        assert!(errors(r#"fn bad() -> Number { return 0 }"#)
            .iter().any(|e| e.code == "missing-test"));
    }

    #[test]
    fn untested_error() {
        let e = errors(r#"fn v(s: String) -> String, err { if s == "" { return err.missing } return s test { self("ok") == "ok" } }"#);
        assert!(e.iter().any(|e| e.code == "untested-error"), "expected untested-error, got: {:?}", e);
    }

    #[test]
    fn missing_success_case() {
        let e = errors(r#"fn v(s: String) -> String, err { if s == "" { return err.missing } return s test { self("") is err.missing } }"#);
        assert!(e.iter().any(|e| e.code == "no-success-test"), "expected no-success-test, got: {:?}", e);
    }

    #[test]
    fn all_covered() {
        let e = errors(r#"fn v(s: String) -> String, err { if s == "" { return err.missing } return s test { self("ok") == "ok" self("") is err.missing } }"#);
        assert!(!e.iter().any(|e| e.code == "untested-error" || e.code == "no-success-test"));
    }

    #[test]
    fn empty_test_block_allowed() {
        let e = errors(r#"fn foo() -> Number { return 0 test {} }"#);
        assert!(!e.iter().any(|e| e.code == "missing-test"), "empty test block should count as present: {:?}", e);
        assert!(!e.iter().any(|e| e.code == "untested-error"), "no errors to test: {:?}", e);
    }

    #[test]
    fn struct_method_errors_from_signature() {
        // Errors declared in struct contract block (signature) should require test coverage
        let e = errors(r#"
            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err missing = "required"
                    err invalid = "invalid"
                }
            }{
                fn validate(raw: String) -> Email, err {
                    if raw == "" { return err.missing }
                    if raw == "x" { return err.invalid }
                    return Email { value: raw }
                    test {
                        self("a@b.com") is Ok
                        self("") is err.missing
                    }
                }
            }
        "#);
        // invalid is declared in the signature but not tested
        assert!(e.iter().any(|e| e.code == "untested-error"), "expected untested-error for 'invalid', got: {:?}", e);
    }

    #[test]
    fn satisfies_method_needs_test() {
        let e = errors(r#"
            contract Stringable {
                to_string() -> String
            }
            pub struct Email {
                value: String
            }{}
            Email satisfies Stringable {
                fn to_string() -> String {
                    return self.value
                }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "missing-test"), "expected missing-test for satisfies method, got: {:?}", e);
    }

    #[test]
    fn struct_test_without_mock() {
        let e = errors(r#"
            pub struct User {
                name: String
                greet() -> String
            }{
                fn greet() -> String {
                    return self.name
                    test {}
                }
            }
            pub fn make() -> User {
                return User { name: "test" }
                test { self() == User { name: "test" } }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "missing-test"),
            "struct with test should pass, got: {:?}", e);
    }

    #[test]
    fn extern_contract_compiles_without_mock() {
        let e = errors(r#"
            extern contract Store {
                getAll() -> String, err {
                    err fail = "fail"
                }
            }
            pub struct Env { store: Store }{}
            pub fn go(env: Env) -> String {
                const data = wait env.store.getAll()
                return data
                crash { env.store.getAll -> halt }
                test { self(Env { store: Store }) is Ok }
            }
        "#);
        // No test-related errors should fire for this valid code
        let test_errs: Vec<_> = e.iter().filter(|e| e.code == "missing-test").collect();
        assert!(test_errs.is_empty(), "should not require mock block: {:?}", test_errs);
    }

    #[test]
    fn multiple_error_paths_all_tested() {
        let e = errors(r#"
            fn validate(s: String) -> String, err {
                if s == "" { return err.empty }
                if s == "bad" { return err.invalid }
                if s == "long" { return err.too_long }
                return s
                test {
                    self("ok") == "ok"
                    self("") is err.empty
                    self("bad") is err.invalid
                    self("long") is err.too_long
                }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "untested-error"), "all errors should be tested: {:?}", e);
        assert!(!e.iter().any(|e| e.code == "no-success-test"), "success case present: {:?}", e);
    }
}

pub struct TestsRule;

impl Rule for TestsRule {
    fn name(&self) -> &'static str { "tests" }

    fn check_function(&self, ctx: &FnCheckContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        let f = ctx.func.def;

        if f.test.is_none() {
            errors.push(RuleError::new(errors::MISSING_TEST, format!("function '{}' has no test block", f.name), Some(ctx.func.qualified_name.clone())));
            return errors;
        }

        // Find declared errors — from struct signature if available, else from function
        let declared_errors = if let Some(parent) = ctx.func.parent_struct {
            // Look up the struct's signature errors for this method
            ctx.check.file.items.iter().find_map(|item| {
                if let Item::Struct(s) = item {
                    if s.name == parent {
                        return s.signatures.iter()
                            .find(|sig| sig.name == f.name)
                            .map(|sig| &sig.errors);
                    }
                }
                None
            }).unwrap_or(&f.errors)
        } else {
            &f.errors
        };

        check_test_coverage(f, declared_errors, &ctx.func.qualified_name, &mut errors);
        // Mock refs removed — auto-stubs replace manual mocks
        errors
    }
}

fn check_test_coverage(f: &FnDef, declared_errors: &[ErrDecl], qn: &str, errors: &mut Vec<RuleError>) {
    let test = match &f.test {
        Some(t) => t,
        None => return,
    };

    if test.cases.is_empty() {
        return;
    }

    // Check test expected values match return type
    for (i, case) in test.cases.iter().enumerate() {
        if let TestCase::Equals { expected, .. } = case {
            if let Some(mismatch) = check_shape(&f.return_type, expected) {
                errors.push(RuleError::new(
                    errors::TEST_SHAPE_MISMATCH,
                    format!("test case {} expected {} but function returns {}", i, mismatch, type_ref_to_name(&f.return_type)),
                    Some(qn.to_string()),
                ));
            }
        }
    }

    let mut all_error_names: HashSet<String> = declared_errors.iter().map(|e| e.name.clone()).collect();
    for name in collect_returned_error_names(&f.body) {
        all_error_names.insert(name);
    }

    if all_error_names.is_empty() && !f.returns_err {
        return;
    }

    let tested_errors: HashSet<String> = test.cases.iter().filter_map(|c| {
        if let TestCase::IsErr { err_name, .. } = c { Some(err_name.clone()) } else { None }
    }).collect();

    for err_name in &all_error_names {
        if !tested_errors.contains(err_name) {
            errors.push(RuleError::new(errors::UNTESTED_ERROR, format!("error '{}' is not tested", err_name), Some(qn.to_string())));
        }
    }

    let has_success = test.cases.iter().any(|c| matches!(c, TestCase::Equals { .. } | TestCase::IsOk { .. }));
    if !has_success && !all_error_names.is_empty() {
        errors.push(RuleError::new(errors::NO_SUCCESS_TEST, "test block has error cases but no success case", Some(qn.to_string())));
    }
}

/// Check if an expected expression matches the function's return type.
/// Returns None if types match, or a description of the mismatch.
fn check_shape(return_type: &TypeRef, expected: &Expr) -> Option<String> {
    match (return_type, expected) {
        (TypeRef::Number, Expr::Number(_)) => None,
        (TypeRef::String, Expr::String(_)) => None,
        (TypeRef::Bool, Expr::Bool(_)) => None,
        // String concat produces String
        (TypeRef::String, Expr::BinOp { op: BinOp::Add, .. }) => None,
        // Struct literals match Named types
        (TypeRef::Named(_), Expr::StructLit { .. }) => None,
        // Null matches nullable and named types
        (TypeRef::Nullable(_), Expr::Null) => None,
        (TypeRef::Named(_), Expr::Null) => None,
        // Array literals match generic Array
        (TypeRef::Generic(name, _), Expr::Array(_)) if name == "Array" => None,
        // Ok type matches null/bool
        (TypeRef::Ok, Expr::Null) => None,
        // Identifiers and complex expressions — can't statically check
        (_, Expr::Ident(_)) => None,
        (_, Expr::Call { .. }) => None,
        (_, Expr::FieldAccess { .. }) => None,
        (_, Expr::BinOp { .. }) => None,
        (_, Expr::Match { .. }) => None,
        // Null against non-nullable primitives
        (TypeRef::Number, Expr::Null) => Some("null".into()),
        (TypeRef::String, Expr::Null) => Some("null".into()),
        (TypeRef::Bool, Expr::Null) => Some("null".into()),
        // Literal type mismatches
        (TypeRef::Number, Expr::String(s)) => Some(format!("String \"{}\"", s)),
        (TypeRef::Number, Expr::Bool(b)) => Some(format!("Bool {}", b)),
        (TypeRef::String, Expr::Number(n)) => Some(format!("Number {}", n)),
        (TypeRef::String, Expr::Bool(b)) => Some(format!("Bool {}", b)),
        (TypeRef::Bool, Expr::Number(n)) => Some(format!("Number {}", n)),
        (TypeRef::Bool, Expr::String(s)) => Some(format!("String \"{}\"", s)),
        _ => None,
    }
}


