use crate::ast::*;
use crate::errors::RuleError;

/// Validate every function has a test block
pub fn check_tests(file: &SourceFile) -> Vec<RuleError> {
    let mut errors = Vec::new();

    // Check standalone functions
    for item in &file.items {
        match item {
            Item::Function(f) => {
                check_fn_has_test(f, None, &mut errors);
            }
            Item::Struct(s) => {
                for method in &s.methods {
                    check_fn_has_test(method, Some(&s.name), &mut errors);
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
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "missing-test");
    }

    #[test]
    fn struct_method_without_test_fails() {
        let file = parse::parse(r#"
            struct Price {
                amount: Number
                add(other: Price) -> Price
            }{
                fn add(other: Price) -> Price {
                    return self
                }
            }
        "#);
        let errors = check_tests(&file);
        assert_eq!(errors.len(), 1);
    }
}
