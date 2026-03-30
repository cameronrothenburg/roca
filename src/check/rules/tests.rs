use std::collections::HashSet;
use crate::ast::*;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::FnCheckContext;

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
    fn invalid_mock_ref_caught() {
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
                test { self() == __mock_User }
            }
        "#);
        assert!(e.iter().any(|e| e.code == "invalid-mock-ref"),
            "expected invalid-mock-ref for struct without mock, got: {:?}", e);
    }

    #[test]
    fn valid_mock_ref_allowed() {
        let e = errors(r#"
            extern contract Store {
                getAll() -> String, err {
                    err fail = "fail"
                }
                mock { getAll -> "[]" }
            }
            pub struct Env { store: Store }{}
            pub fn go(env: Env) -> String {
                const data = wait env.store.getAll()
                return data
                crash { env.store.getAll -> halt }
                test { self(Env { store: __mock_Store }) is Ok }
            }
        "#);
        assert!(!e.iter().any(|e| e.code == "invalid-mock-ref"),
            "valid mock ref should pass, got: {:?}", e);
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
            errors.push(RuleError {
                code: errors::MISSING_TEST.into(),
                message: format!("function '{}' has no test block", f.name),
                context: Some(ctx.func.qualified_name.clone()),
            });
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
        check_mock_refs(f, ctx.check.file, &ctx.func.qualified_name, &mut errors);
        errors
    }
}

fn check_test_coverage(f: &FnDef, declared_errors: &[ErrDecl], qn: &str, errors: &mut Vec<RuleError>) {
    let test = match &f.test {
        Some(t) => t,
        None => return,
    };

    let mut all_error_names: HashSet<String> = declared_errors.iter().map(|e| e.name.clone()).collect();
    for name in collect_returned_error_names(&f.body) {
        all_error_names.insert(name);
    }

    if all_error_names.is_empty() && !f.returns_err {
        return;
    }

    if test.cases.is_empty() {
        return;
    }

    let tested_errors: HashSet<String> = test.cases.iter().filter_map(|c| {
        if let TestCase::IsErr { err_name, .. } = c { Some(err_name.clone()) } else { None }
    }).collect();

    for err_name in &all_error_names {
        if !tested_errors.contains(err_name) {
            errors.push(RuleError {
                code: errors::UNTESTED_ERROR.into(),
                message: format!("error '{}' is not tested", err_name),
                context: Some(qn.to_string()),
            });
        }
    }

    let has_success = test.cases.iter().any(|c| matches!(c, TestCase::Equals { .. } | TestCase::IsOk { .. }));
    if !has_success && !all_error_names.is_empty() {
        errors.push(RuleError {
            code: errors::NO_SUCCESS_TEST.into(),
            message: "test block has error cases but no success case".into(),
            context: Some(qn.to_string()),
        });
    }
}

fn check_mock_refs(f: &FnDef, file: &SourceFile, qn: &str, errors: &mut Vec<RuleError>) {
    let test = match &f.test {
        Some(t) => t,
        None => return,
    };

    let mut valid_mocks: HashSet<String> = file.items.iter().filter_map(|item| {
        match item {
            Item::Contract(c) if c.mock.is_some() => Some(c.name.clone()),
            Item::ExternContract(c) if c.mock.is_some() => Some(c.name.clone()),
            _ => None,
        }
    }).collect();

    // Also check imported files for mock blocks
    for item in &file.items {
        if let Item::Import(imp) = item {
            if let ImportSource::Path(path) = &imp.source {
                let roca_path = std::path::Path::new(path);
                for base in &[".", "src"] {
                    let full_path = std::path::Path::new(base).join(roca_path);
                    if let Ok(source) = std::fs::read_to_string(&full_path) {
                        if let Ok(imported) = crate::parse::try_parse(&source) {
                            for imp_item in &imported.items {
                                match imp_item {
                                    Item::Contract(c) if c.mock.is_some() => { valid_mocks.insert(c.name.clone()); }
                                    Item::ExternContract(c) if c.mock.is_some() => { valid_mocks.insert(c.name.clone()); }
                                    _ => {}
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    let mut mock_refs = Vec::new();
    for case in &test.cases {
        match case {
            TestCase::Equals { args, expected } => {
                for arg in args { collect_mock_idents(arg, &mut mock_refs); }
                collect_mock_idents(expected, &mut mock_refs);
            }
            TestCase::IsOk { args } | TestCase::IsErr { args, .. } => {
                for arg in args { collect_mock_idents(arg, &mut mock_refs); }
            }
            TestCase::StatusMock { mocks, .. } => {
                for m in mocks { collect_mock_idents(&m.value, &mut mock_refs); }
            }
        }
    }

    for mock_ref in &mock_refs {
        if !valid_mocks.contains(mock_ref.as_str()) {
            errors.push(RuleError {
                code: errors::INVALID_MOCK_REF.into(),
                message: format!("__mock_{} used but '{}' has no mock block — only contracts with mock {{}} can be mocked", mock_ref, mock_ref),
                context: Some(qn.to_string()),
            });
        }
    }
}

fn collect_mock_idents(expr: &Expr, refs: &mut Vec<String>) {
    match expr {
        Expr::Ident(name) if name.starts_with("__mock_") => {
            let contract_name = name.strip_prefix("__mock_").unwrap().to_string();
            if !refs.contains(&contract_name) {
                refs.push(contract_name);
            }
        }
        Expr::StructLit { fields, .. } => {
            for (_, v) in fields {
                collect_mock_idents(v, refs);
            }
        }
        Expr::Call { target, args } => {
            collect_mock_idents(target, refs);
            for a in args { collect_mock_idents(a, refs); }
        }
        Expr::Array(elements) => {
            for e in elements { collect_mock_idents(e, refs); }
        }
        _ => {}
    }
}

