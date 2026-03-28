use std::collections::HashSet;
use crate::ast::*;
use crate::errors::RuleError;

/// Validate contract definitions in isolation
pub fn check_contracts(file: &SourceFile) -> Vec<RuleError> {
    let mut errors = Vec::new();

    for item in &file.items {
        if let Item::Contract(c) = item {
            // Check: err names unique within contract
            let mut seen_errs = HashSet::new();
            for func in &c.functions {
                for err in &func.errors {
                    if !seen_errs.insert(&err.name) {
                        errors.push(RuleError {
                            code: "duplicate-err".into(),
                            message: format!("duplicate error name '{}' in contract '{}'", err.name, c.name),
                            context: Some(format!("in {}.{}", c.name, func.name)),
                        });
                    }
                }
            }

            // Check: fn signatures have return types (they always do via parser, but validate)
            for func in &c.functions {
                if func.returns_err && func.errors.is_empty() {
                    errors.push(RuleError {
                        code: "err-no-errors".into(),
                        message: format!("function '{}' returns err but declares no error names", func.name),
                        context: Some(format!("in contract '{}'", c.name)),
                    });
                }
            }
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn no_errors_on_valid_contract() {
        let file = parse::parse("contract Stringable { to_string() -> String }");
        let errors = check_contracts(&file);
        assert!(errors.is_empty());
    }

    #[test]
    fn duplicate_err_name() {
        let file = parse::parse(r#"
            contract Bad {
                get(url: String) -> String, err {
                    err timeout = "a"
                    err timeout = "b"
                }
            }
        "#);
        let errors = check_contracts(&file);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "duplicate-err");
    }
}
