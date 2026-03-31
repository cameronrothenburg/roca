//! Rule: duplicate-err, err-no-errors, mock-null
//! Validates contract error declarations and mock blocks.

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
                            errors.push(RuleError::new(errors::DUPLICATE_ERR, format!("duplicate error name '{}' in contract '{}'", err.name, c.name), Some(format!("in {}.{}", c.name, func.name))));
                        }
                    }
                    if func.returns_err && func.errors.is_empty() {
                        errors.push(RuleError::new(errors::ERR_NO_ERRORS, format!("function '{}' returns err but declares no error names", func.name), Some(format!("in contract '{}'", c.name))));
                    }
                }
            }
            _ => {}
        }
        errors
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
        let ctx = CheckContext { file: &file, registry: &reg, source_dir: None };
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
    fn extern_contract_no_mock_needed() {
        // mock blocks are auto-generated — no user mock block required
        let errors = check(r#"extern contract Good { get() -> String }"#);
        assert!(errors.is_empty(), "extern contract should not require mock: {:?}", errors);
    }
}
