use std::collections::HashMap;
use crate::ast::*;

#[derive(Debug, Clone)]
pub struct ResolvedContract {
    pub name: String,
    pub functions: Vec<FnSignature>,
    pub fields: Vec<Field>,
    pub errors: HashMap<String, String>,
    pub has_mock: bool,
    pub values: Vec<ContractValue>,
}

#[derive(Debug)]
pub struct ContractRegistry {
    pub contracts: HashMap<String, ResolvedContract>,
    pub struct_contracts: HashMap<String, ResolvedContract>,
    /// Map of type → contracts it satisfies. e.g. Email → [String, Loggable]
    pub satisfies_map: HashMap<String, Vec<String>>,
}

const STDLIB_SOURCE: &str = include_str!("../../stdlib/primitives.roca");

impl ContractRegistry {
    /// Build the registry from a parsed source file, with stdlib loaded
    pub fn build(file: &SourceFile) -> Self {
        let mut reg = Self {
            contracts: HashMap::new(),
            struct_contracts: HashMap::new(),
            satisfies_map: HashMap::new(),
        };

        let stdlib = crate::parse::parse(STDLIB_SOURCE);
        reg.load_file(&stdlib);
        reg.load_file(file);

        reg
    }

    pub fn load_file(&mut self, file: &SourceFile) {
        for item in &file.items {
            match item {
                Item::Contract(c) => {
                    let mut all_errors = HashMap::new();
                    for func in &c.functions {
                        for err in &func.errors {
                            all_errors.insert(err.name.clone(), err.message.clone());
                        }
                    }
                    self.contracts.insert(c.name.clone(), ResolvedContract {
                        name: c.name.clone(),
                        functions: c.functions.clone(),
                        fields: c.fields.clone(),
                        errors: all_errors,
                        has_mock: c.mock.is_some(),
                        values: c.values.clone(),
                    });
                }
                Item::Struct(s) => {
                    let mut all_errors = HashMap::new();
                    for sig in &s.signatures {
                        for err in &sig.errors {
                            all_errors.insert(err.name.clone(), err.message.clone());
                        }
                    }
                    self.struct_contracts.insert(s.name.clone(), ResolvedContract {
                        name: s.name.clone(),
                        functions: s.signatures.clone(),
                        fields: s.fields.clone(),
                        errors: all_errors,
                        has_mock: false,
                        values: Vec::new(),
                    });
                }
                Item::Satisfies(sat) => {
                    self.satisfies_map
                        .entry(sat.struct_name.clone())
                        .or_default()
                        .push(sat.contract_name.clone());
                }
                _ => {}
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<&ResolvedContract> {
        self.contracts.get(name).or_else(|| self.struct_contracts.get(name))
    }

    /// Check if a type satisfies a contract (e.g. Email satisfies String)
    pub fn type_satisfies(&self, type_name: &str, contract_name: &str) -> bool {
        // Same type — always satisfies itself
        if type_name == contract_name {
            return true;
        }
        if let Some(contracts) = self.satisfies_map.get(type_name) {
            contracts.iter().any(|c| c == contract_name)
        } else {
            false
        }
    }

    /// Check if a type is acceptable where `expected` is required
    /// i.e. type_name == expected OR type_name satisfies expected
    pub fn type_accepts(&self, expected: &str, actual: &str) -> bool {
        if expected == actual {
            return true;
        }
        self.type_satisfies(actual, expected)
    }

    /// Check if a type has a specific method or field
    /// Checks own contract + all contracts it satisfies
    pub fn has_method(&self, type_name: &str, method_name: &str) -> bool {
        // Check own methods/fields
        if let Some(contract) = self.get(type_name) {
            if contract.functions.iter().any(|f| f.name == method_name) {
                return true;
            }
            if contract.fields.iter().any(|f| f.name == method_name) {
                return true;
            }
        }
        // Check methods from satisfied contracts
        if let Some(satisfied) = self.satisfies_map.get(type_name) {
            for contract_name in satisfied {
                if let Some(contract) = self.get(contract_name) {
                    if contract.functions.iter().any(|f| f.name == method_name) {
                        return true;
                    }
                    if contract.fields.iter().any(|f| f.name == method_name) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Get all available methods for a type (for error messages)
    pub fn available_methods(&self, type_name: &str) -> Vec<String> {
        let mut methods = Vec::new();
        if let Some(contract) = self.get(type_name) {
            for f in &contract.functions {
                methods.push(f.name.clone());
            }
            for f in &contract.fields {
                methods.push(f.name.clone());
            }
        }
        // Include methods from satisfied contracts
        if let Some(satisfied) = self.satisfies_map.get(type_name) {
            for contract_name in satisfied {
                if let Some(contract) = self.get(contract_name) {
                    for f in &contract.functions {
                        if !methods.contains(&f.name) {
                            methods.push(f.name.clone());
                        }
                    }
                }
            }
        }
        methods
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn stdlib_loaded() {
        let file = parse::parse("");
        let reg = ContractRegistry::build(&file);
        assert!(reg.get("String").is_some());
        assert!(reg.get("Number").is_some());
        assert!(reg.get("Bool").is_some());
        assert!(reg.get("Array").is_some());
        assert!(reg.get("Bytes").is_some());
    }

    #[test]
    fn string_has_trim() {
        let file = parse::parse("");
        let reg = ContractRegistry::build(&file);
        assert!(reg.has_method("String", "trim"));
        assert!(reg.has_method("String", "includes"));
        assert!(reg.has_method("String", "toUpperCase"));
        assert!(!reg.has_method("String", "nonexistent"));
    }

    #[test]
    fn number_has_to_string() {
        let file = parse::parse("");
        let reg = ContractRegistry::build(&file);
        assert!(reg.has_method("Number", "toString"));
        assert!(reg.has_method("Number", "toFixed"));
        assert!(!reg.has_method("Number", "trim"));
    }

    #[test]
    fn user_contract_merged() {
        let file = parse::parse(r#"
            contract HttpClient {
                get(url: String) -> String, err {
                    err timeout = "timed out"
                }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        assert!(reg.get("String").is_some());
        assert!(reg.get("HttpClient").is_some());
    }

    #[test]
    fn available_methods_list() {
        let file = parse::parse("");
        let reg = ContractRegistry::build(&file);
        let methods = reg.available_methods("String");
        assert!(methods.contains(&"trim".to_string()));
        assert!(methods.contains(&"includes".to_string()));
    }

    #[test]
    fn satisfies_tracked() {
        let file = parse::parse(r#"
            contract Loggable { to_log() -> String }
            pub struct Email { value: String }{}
            Email satisfies Loggable {
                fn to_log() -> String { return self.value test {} }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        assert!(reg.type_satisfies("Email", "Loggable"));
        assert!(!reg.type_satisfies("Email", "String"));
    }

    #[test]
    fn type_accepts_same() {
        let file = parse::parse("");
        let reg = ContractRegistry::build(&file);
        assert!(reg.type_accepts("String", "String"));
        assert!(reg.type_accepts("Number", "Number"));
    }

    #[test]
    fn type_accepts_via_satisfies() {
        let file = parse::parse(r#"
            pub struct Email { value: String }{}
            Email satisfies String {
                fn trim() -> String { return self.value test {} }
            }
        "#);
        let reg = ContractRegistry::build(&file);
        assert!(reg.type_accepts("String", "Email"));
        assert!(!reg.type_accepts("Number", "Email"));
    }
}
