use std::collections::HashSet;
use crate::ast::*;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::ItemContext;

pub struct ContractsRule;

impl Rule for ContractsRule {
    fn name(&self) -> &'static str { "contracts" }

    fn check_item(&self, ctx: &ItemContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        match ctx.item {
            Item::Contract(c) | Item::ExternContract(c) => {
                let mut seen_errs = HashSet::new();
                for func in &c.functions {
                    for err in &func.errors {
                        if !seen_errs.insert(&err.name) {
                            errors.push(RuleError {
                                code: errors::DUPLICATE_ERR.into(),
                                message: format!("duplicate error name '{}' in contract '{}'", err.name, c.name),
                                context: Some(format!("in {}.{}", c.name, func.name)),
                            });
                        }
                    }
                    if func.returns_err && func.errors.is_empty() {
                        errors.push(RuleError {
                            code: errors::ERR_NO_ERRORS.into(),
                            message: format!("function '{}' returns err but declares no error names", func.name),
                            context: Some(format!("in contract '{}'", c.name)),
                        });
                    }
                }
                if let Some(mock) = &c.mock {
                    check_mock_values(&c.name, mock, &mut errors);
                }
            }
            _ => {}
        }
        errors
    }
}

fn check_mock_values(contract_name: &str, mock: &MockDef, errors: &mut Vec<RuleError>) {
    for entry in &mock.entries {
        if matches!(entry.value, Expr::Null) {
            errors.push(RuleError {
                code: errors::MOCK_NULL.into(),
                message: format!("mock for '{}.{}' cannot return null — provide a valid mock value", contract_name, entry.method),
                context: Some(format!("in {} mock block", contract_name)),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::context::CheckContext;
    use crate::check::registry::ContractRegistry;

    fn check(src: &str) -> Vec<RuleError> {
        let file = crate::parse::parse(src);
        let reg = ContractRegistry::build(&file);
        let rule = ContractsRule;
        let ctx = CheckContext { file: &file, registry: &reg };
        file.items.iter().flat_map(|item| {
            rule.check_item(&ItemContext { check: &ctx, item })
        }).collect()
    }

    #[test]
    fn no_errors_on_valid() {
        assert!(check("contract Stringable { to_string() -> String }").is_empty());
    }

    #[test]
    fn duplicate_err_name() {
        let errors = check(r#"contract Bad { get(url: String) -> String, err { err timeout = "a" err timeout = "b" } }"#);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "duplicate-err");
    }

    #[test]
    fn err_no_errors_caught() {
        let errors = check(r#"contract Bad { get() -> String, err }"#);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "err-no-errors");
    }

    #[test]
    fn valid_contract_with_errors() {
        let errors = check(r#"contract Good { get() -> String, err { err not_found = "missing" } }"#);
        assert!(errors.is_empty());
    }

    #[test]
    fn mock_null_blocked() {
        let errors = check(r#"contract Bad { get() -> String mock { get -> null } }"#);
        assert!(errors.iter().any(|e| e.code == "mock-null"),
            "expected mock-null, got: {:?}", errors);
    }

    #[test]
    fn mock_valid_value_allowed() {
        let errors = check(r#"contract Good { get() -> String mock { get -> "test" } }"#);
        assert!(!errors.iter().any(|e| e.code == "mock-null"),
            "valid mock should pass, got: {:?}", errors);
    }

    #[test]
    fn extern_contract_mock_null_blocked() {
        let errors = check(r#"extern contract Bad { get() -> String mock { get -> null } }"#);
        assert!(errors.iter().any(|e| e.code == "mock-null"),
            "expected mock-null on extern contract, got: {:?}", errors);
    }
}
