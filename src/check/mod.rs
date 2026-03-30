//! Static analysis and rule checking for Roca source files.
//! Orchestrates all rules and walks the AST to produce diagnostics.

pub mod context;
pub mod walker;
pub mod rule;
pub mod registry;
pub mod rules;

use crate::ast::SourceFile;
use crate::errors::RuleError;
use registry::ContractRegistry;
use rule::Rule;

/// All registered rules — add new rules here
fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(rules::contracts::ContractsRule),
        Box::new(rules::constraints::ConstraintsRule),
        Box::new(rules::structs::StructsRule),
        Box::new(rules::satisfies::SatisfiesRule),
        Box::new(rules::crash::CrashRule),
        Box::new(rules::tests::TestsRule),
        Box::new(rules::variables::VariablesRule),
        Box::new(rules::methods::MethodsRule),
        Box::new(rules::types::TypeCheckRule),
        Box::new(rules::unhandled::UnhandledErrorsRule),
        Box::new(rules::manual_err::NoManualErrRule),
        Box::new(rules::docs::DocsRule),
    ]
}

pub fn check(file: &SourceFile) -> Vec<RuleError> {
    let registry = ContractRegistry::build(file);
    check_with_registry(file, &registry)
}

pub fn check_with_registry(file: &SourceFile, registry: &ContractRegistry) -> Vec<RuleError> {
    walker::walk(file, registry, &all_rules())
}

#[cfg(test)]
mod check_tests {
    use super::*;
    use crate::parse;

    #[test]
    fn valid_program_passes_all_checks() {
        let file = parse::parse(r#"
            contract Stringable {
                to_string() -> String
            }

            /// An email address
            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err missing = "required"
                }
            }{
                fn validate(raw: String) -> Email, err {
                    if raw == "" { return err.missing }
                    return Email { value: raw }
                    test {
                        self("a@b.com") is Ok
                        self("") is err.missing
                    }
                }
            }

            Email satisfies Stringable {
                fn to_string() -> String {
                    return self.value
                    test { self() == "test" }
                }
            }

            /// Greets a person by name
            pub fn greet(name: String) -> String {
                let trimmed = name.trim()
                return "Hello " + trimmed
                crash { name.trim -> halt }
                test { self("cam") == "Hello cam" }
            }
        "#);
        let errors = check(&file);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn multiple_errors_collected() {
        let file = parse::parse(r#"
            fn bad() -> Number {
                const x = 5
                x = 10
                let y = x.to_string()
                return 0
            }
        "#);
        let errors = check(&file);
        assert!(errors.len() >= 2);
    }
}
