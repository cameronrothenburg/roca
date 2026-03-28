use std::collections::HashMap;
use crate::ast::*;

/// Resolved contract — all info the checker needs about a contract
#[derive(Debug, Clone)]
pub struct ResolvedContract {
    pub name: String,
    pub functions: Vec<FnSignature>,
    pub fields: Vec<Field>,
    pub errors: HashMap<String, String>,
    pub has_mock: bool,
    pub values: Vec<ContractValue>,
}

/// Registry of all contracts in a source file
#[derive(Debug)]
pub struct ContractRegistry {
    pub contracts: HashMap<String, ResolvedContract>,
    /// Struct inline contracts (from first {} block)
    pub struct_contracts: HashMap<String, ResolvedContract>,
}

impl ContractRegistry {
    /// Build the registry from a parsed source file
    pub fn build(file: &SourceFile) -> Self {
        let mut contracts = HashMap::new();
        let mut struct_contracts = HashMap::new();

        for item in &file.items {
            match item {
                Item::Contract(c) => {
                    // Collect all errors across all functions
                    let mut all_errors = HashMap::new();
                    for func in &c.functions {
                        for err in &func.errors {
                            all_errors.insert(err.name.clone(), err.message.clone());
                        }
                    }

                    contracts.insert(c.name.clone(), ResolvedContract {
                        name: c.name.clone(),
                        functions: c.functions.clone(),
                        fields: c.fields.clone(),
                        errors: all_errors,
                        has_mock: c.mock.is_some(),
                        values: c.values.clone(),
                    });
                }
                Item::Struct(s) => {
                    // Build a contract from the struct's first {} block
                    let mut all_errors = HashMap::new();
                    for sig in &s.signatures {
                        for err in &sig.errors {
                            all_errors.insert(err.name.clone(), err.message.clone());
                        }
                    }

                    struct_contracts.insert(s.name.clone(), ResolvedContract {
                        name: s.name.clone(),
                        functions: s.signatures.clone(),
                        fields: s.fields.clone(),
                        errors: all_errors,
                        has_mock: false,
                        values: Vec::new(),
                    });
                }
                _ => {}
            }
        }

        ContractRegistry { contracts, struct_contracts }
    }

    /// Look up a contract by name (checks both contracts and struct contracts)
    pub fn get(&self, name: &str) -> Option<&ResolvedContract> {
        self.contracts.get(name).or_else(|| self.struct_contracts.get(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn build_registry() {
        let file = parse::parse(r#"
            contract Stringable { to_string() -> String }
            contract HttpClient {
                get(url: String) -> Response, err {
                    err timeout = "timed out"
                }
                mock { get -> Ok }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        assert!(reg.get("Stringable").is_some());
        assert!(reg.get("HttpClient").is_some());
        let http = reg.get("HttpClient").unwrap();
        assert!(http.has_mock);
        assert!(http.errors.contains_key("timeout"));
    }

    #[test]
    fn build_struct_contract() {
        let file = parse::parse(r#"
            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err missing = "required"
                }
            }{
                fn validate(raw: String) -> Email, err {
                    return Email { value: raw }
                    test { self("a") is Ok }
                }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        assert!(reg.get("Email").is_some());
        let email = reg.get("Email").unwrap();
        assert_eq!(email.functions.len(), 1);
        assert!(email.errors.contains_key("missing"));
    }
}
