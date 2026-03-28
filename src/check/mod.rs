pub mod registry;
pub mod contracts;
pub mod structs;
pub mod satisfies;
pub mod crash;
pub mod tests;
pub mod variables;
pub mod methods;

use crate::ast::SourceFile;
use crate::errors::RuleError;
use registry::ContractRegistry;

/// Run all checks on a source file, in dependency order
pub fn check(file: &SourceFile) -> Vec<RuleError> {
    let registry = ContractRegistry::build(file);
    check_with_registry(file, &registry)
}

/// Run all checks using a pre-built registry (for cross-module resolution)
pub fn check_with_registry(file: &SourceFile, registry: &ContractRegistry) -> Vec<RuleError> {
    let mut errors = Vec::new();

    // 2. Validate contracts in isolation
    errors.extend(contracts::check_contracts(file));

    // 3. Validate struct impl matches contract block
    errors.extend(structs::check_structs(file));

    // 4. Validate satisfies fulfills external contract
    errors.extend(satisfies::check_satisfies(file, &registry));

    // 5. Validate crash blocks cover all calls
    errors.extend(crash::check_crash(file));

    // 6. Validate all functions have test blocks
    errors.extend(tests::check_tests(file));

    // 7. Validate const/let rules
    errors.extend(variables::check_variables(file));

    // 8. Validate method calls exist on types
    errors.extend(methods::check_methods(file, &registry));

    errors
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

            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err missing = "required"
                    err invalid = "invalid"
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
        // Should get: missing-test, const-reassign, missing-crash
        assert!(errors.len() >= 2);
    }
}
