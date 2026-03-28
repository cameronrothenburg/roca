use crate::ast::*;
use crate::errors::RuleError;
use super::registry::ContractRegistry;

/// Validate satisfies blocks fulfill their contracts
pub fn check_satisfies(file: &SourceFile, registry: &ContractRegistry) -> Vec<RuleError> {
    let mut errors = Vec::new();

    for item in &file.items {
        if let Item::Satisfies(sat) = item {
            // Contract must exist
            let contract = match registry.contracts.get(&sat.contract_name) {
                Some(c) => c,
                None => {
                    errors.push(RuleError {
                        code: "unknown-contract".into(),
                        message: format!(
                            "'{}' satisfies '{}' but contract '{}' does not exist",
                            sat.struct_name, sat.contract_name, sat.contract_name
                        ),
                        context: None,
                    });
                    continue;
                }
            };

            // Every function in the contract must be implemented
            for sig in &contract.functions {
                let method = sat.methods.iter().find(|m| m.name == sig.name);
                match method {
                    None => {
                        errors.push(RuleError {
                            code: "missing-satisfies".into(),
                            message: format!(
                                "'{}' satisfies '{}' but does not implement '{}'",
                                sat.struct_name, sat.contract_name, sig.name
                            ),
                            context: None,
                        });
                    }
                    Some(m) => {
                        // Check param count
                        if sig.params.len() != m.params.len() {
                            errors.push(RuleError {
                                code: "satisfies-mismatch".into(),
                                message: format!(
                                    "'{}.{}' has {} params but '{}' requires {}",
                                    sat.struct_name, m.name, m.params.len(),
                                    sat.contract_name, sig.params.len()
                                ),
                                context: None,
                            });
                        }
                        // Check return type
                        if sig.return_type != m.return_type {
                            errors.push(RuleError {
                                code: "satisfies-mismatch".into(),
                                message: format!(
                                    "'{}.{}' returns {:?} but '{}' requires {:?}",
                                    sat.struct_name, m.name, m.return_type,
                                    sat.contract_name, sig.return_type
                                ),
                                context: None,
                            });
                        }
                    }
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
    fn valid_satisfies() {
        let file = parse::parse(r#"
            contract Stringable { to_string() -> String }
            pub struct Email { value: String }{
            }
            Email satisfies Stringable {
                fn to_string() -> String {
                    return self.value
                    test { self() == "test" }
                }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        let errors = check_satisfies(&file, &reg);
        assert!(errors.is_empty());
    }

    #[test]
    fn missing_method_in_satisfies() {
        let file = parse::parse(r#"
            contract Serializable {
                serialize() -> String
                deserialize(raw: String) -> String
            }
            pub struct Email { value: String }{}
            Email satisfies Serializable {
                fn serialize() -> String {
                    return self.value
                    test { self() == "test" }
                }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        let errors = check_satisfies(&file, &reg);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "missing-satisfies");
    }

    #[test]
    fn unknown_contract() {
        let file = parse::parse(r#"
            pub struct Email { value: String }{}
            Email satisfies Nonexistent {
                fn foo() -> String {
                    return "bar"
                    test { self() == "bar" }
                }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        let errors = check_satisfies(&file, &reg);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "unknown-contract");
    }
}
