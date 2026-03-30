use std::collections::HashMap;
use std::sync::LazyLock;
use crate::ast::*;

#[derive(Debug, Clone)]
pub struct ResolvedContract {
    pub type_params: Vec<TypeParam>,
    pub functions: Vec<FnSignature>,
    pub fields: Vec<Field>,
}

#[derive(Debug)]
pub struct ContractRegistry {
    pub contracts: HashMap<String, ResolvedContract>,
    pub struct_contracts: HashMap<String, ResolvedContract>,
    /// Map of type → contracts it satisfies. e.g. Email → [String, Loggable]
    pub satisfies_map: HashMap<String, Vec<String>>,
}

const STDLIB_SOURCE: &str = include_str!("../../packages/stdlib/primitives.roca");

/// Stdlib parsed once and cached for the lifetime of the process.
static STDLIB_AST: LazyLock<SourceFile> = LazyLock::new(|| crate::parse::parse(STDLIB_SOURCE));

/// Stdlib modules — loaded dynamically from the stdlib directory.
/// The module name maps to stdlib/{name}.roca
fn stdlib_module(name: &str) -> Option<String> {
    // Find stdlib directory relative to the roca binary
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;

    // Check alongside binary first, then common install locations
    for base in &[
        exe_dir.join("../packages/stdlib"),
        exe_dir.join("../../packages/stdlib"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/stdlib"),
    ] {
        let path = base.join(format!("{}.roca", name));
        if let Ok(source) = std::fs::read_to_string(&path) {
            return Some(source);
        }
    }
    None
}

impl ContractRegistry {
    /// Build the registry from a parsed source file, with stdlib loaded
    pub fn build(file: &SourceFile) -> Self {
        let mut reg = Self {
            contracts: HashMap::new(),
            struct_contracts: HashMap::new(),
            satisfies_map: HashMap::new(),
        };

        reg.load_file(&STDLIB_AST);
        reg.load_file(file);

        reg
    }

    fn resolve_contract(c: &ContractDef) -> ResolvedContract {
        ResolvedContract {
            type_params: c.type_params.clone(),
            functions: c.functions.clone(),
            fields: c.fields.clone(),
        }
    }

    pub fn load_file(&mut self, file: &SourceFile) {
        for item in &file.items {
            match item {
                Item::Contract(c) | Item::ExternContract(c) => {
                    self.contracts.insert(c.name.clone(), Self::resolve_contract(c));
                }
                Item::Struct(s) => {
                    self.struct_contracts.insert(s.name.clone(), ResolvedContract {
                        type_params: Vec::new(),
                        functions: s.signatures.clone(),
                        fields: s.fields.clone(),
                    });
                }
                Item::Enum(e) => {
                    self.contracts.insert(e.name.clone(), ResolvedContract {
                        type_params: Vec::new(),
                        functions: Vec::new(),
                        fields: Vec::new(),
                    });
                }
                Item::Satisfies(sat) => {
                    self.satisfies_map
                        .entry(sat.struct_name.clone())
                        .or_default()
                        .push(sat.contract_name.clone());
                }
                Item::Import(imp) => {
                    if let ImportSource::Std(Some(module)) = &imp.source {
                        if let Some(src) = stdlib_module(module) {
                            if let Ok(parsed) = crate::parse::try_parse(&src) {
                                // Load only the imported names
                                for imp_item in &parsed.items {
                                    match imp_item {
                                        Item::Contract(c) | Item::ExternContract(c)
                                            if imp.names.iter().any(|n| n == &c.name) =>
                                        {
                                            self.contracts.insert(c.name.clone(), Self::resolve_contract(c));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
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

    /// Extract the base name from a possibly-generic type like "Array<Email>" → "Array"
    fn base_name(type_name: &str) -> &str {
        type_name.split('<').next().unwrap_or(type_name)
    }

    /// Check if a type has a specific method or field
    /// Checks own contract + all contracts it satisfies
    /// Handles generic types: Array<Email> looks up Array contract
    pub fn has_method(&self, type_name: &str, method_name: &str) -> bool {
        let base = Self::base_name(type_name);
        // Check own methods/fields
        if let Some(contract) = self.get(base) {
            if contract.functions.iter().any(|f| f.name == method_name) {
                return true;
            }
            if contract.fields.iter().any(|f| f.name == method_name) {
                return true;
            }
        }
        // Check methods from satisfied contracts
        if let Some(satisfied) = self.satisfies_map.get(base) {
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

    /// Parse generic type args from a type string like "Array<Email>" → ["Email"]
    fn parse_type_args(type_name: &str) -> Vec<&str> {
        if let Some(lt) = type_name.find('<') {
            let args_str = &type_name[lt + 1..type_name.len() - 1];
            args_str.split(", ").collect()
        } else {
            Vec::new()
        }
    }

    /// Get a method signature and its generic substitution map.
    /// Returns None if type/method not found. Subs map is empty for non-generic types.
    pub fn get_method(&self, type_name: &str, method_name: &str) -> Option<(&FnSignature, HashMap<String, String>)> {
        let base = Self::base_name(type_name);
        let contract = self.get(base)?;
        let sig = contract.functions.iter().find(|f| f.name == method_name)?;

        let mut subs = HashMap::new();
        let args = Self::parse_type_args(type_name);
        for (i, tp) in contract.type_params.iter().enumerate() {
            if let Some(arg) = args.get(i) {
                subs.insert(tp.name.clone(), arg.to_string());
            }
        }

        Some((sig, subs))
    }

    /// Check generic constraints on a type. Only call for generic types (contains '<').
    pub fn check_generic_constraints(&self, type_name: &str) -> Vec<(String, String, String)> {
        let mut violations = Vec::new();
        let base = Self::base_name(type_name);
        let contract = match self.get(base) {
            Some(c) if !c.type_params.is_empty() => c,
            _ => return violations,
        };

        let args = Self::parse_type_args(type_name);
        for (i, tp) in contract.type_params.iter().enumerate() {
            if let (Some(constraint), Some(arg)) = (&tp.constraint, args.get(i)) {
                if *arg == constraint.as_str() || self.type_satisfies(arg, constraint) {
                    continue;
                }
                violations.push((
                    arg.to_string(),
                    constraint.clone(),
                    type_name.to_string(),
                ));
            }
        }
        violations
    }

    /// Check if a call target returns errors, given a dotted call name and variable scope.
    /// e.g. "Email.validate" → check Email struct's validate method
    /// e.g. "intl.dateTime" → resolve intl from params to DateFormatting, check dateTime
    /// Returns None if unknown (can't resolve), Some(true/false) if known.
    pub fn call_returns_err_with_scope(&self, call_name: &str, file: &SourceFile, scope: &std::collections::HashMap<String, String>) -> Option<bool> {
        if let Some(dot) = call_name.find('.') {
            let var_name = &call_name[..dot];
            let method_name = &call_name[dot + 1..];

            // Try direct type lookup first (e.g. "Email.validate")
            if let Some(contract) = self.get(var_name) {
                if let Some(sig) = contract.functions.iter().find(|f| f.name == method_name) {
                    return Some(sig.returns_err);
                }
            }

            // Resolve variable name from scope (e.g. "intl" → "DateFormatting", "formatter" → "IntlDateTimeFormat")
            if let Some(type_name) = scope.get(var_name) {
                if let Some(contract) = self.get(type_name.as_str()) {
                    if let Some(sig) = contract.functions.iter().find(|f| f.name == method_name) {
                        return Some(sig.returns_err);
                    }
                }
            }

            // Try resolving through nested field access (e.g. "env.db.query")
            if let Some(returns_err) = self.resolve_nested_field_call(call_name, dot, var_name, scope) {
                return Some(returns_err);
            }
        } else {
            // Top-level function name
            for item in &file.items {
                if let Item::Function(f) = item {
                    if f.name == call_name {
                        return Some(f.returns_err);
                    }
                }
                if let Item::ExternFn(f) = item {
                    if f.name == call_name {
                        return Some(f.returns_err);
                    }
                }
            }
        }
        None
    }

    /// Resolve nested field access like "env.db.query" → find env's type, look up db field,
    /// then find query method on db's type.
    fn resolve_nested_field_call(
        &self,
        call_name: &str,
        first_dot: usize,
        var_name: &str,
        scope: &std::collections::HashMap<String, String>,
    ) -> Option<bool> {
        let rest = &call_name[first_dot + 1..];
        let second_dot = rest.find('.')?;
        let field_name = &rest[..second_dot];
        let inner_method = &rest[second_dot + 1..];

        let type_name = scope.get(var_name)?;
        let contract = self.get(type_name.as_str())?;
        let field = contract.fields.iter().find(|f| f.name == field_name)?;
        let field_type = match &field.type_ref {
            TypeRef::Named(n) => n.as_str(),
            _ => return None,
        };
        let field_contract = self.get(field_type)?;
        let sig = field_contract.functions.iter().find(|f| f.name == inner_method)?;
        Some(sig.returns_err)
    }

    /// Check if a method on a type is public
    pub fn is_method_pub(&self, type_name: &str, method_name: &str) -> bool {
        let base = Self::base_name(type_name);
        if let Some(contract) = self.get(base) {
            if let Some(sig) = contract.functions.iter().find(|f| f.name == method_name) {
                return sig.is_pub;
            }
        }
        // Stdlib/contract methods are always public
        true
    }

    /// Get all available methods for a type (for error messages)
    pub fn available_methods(&self, type_name: &str) -> Vec<String> {
        let base = Self::base_name(type_name);
        let mut methods = Vec::new();
        if let Some(contract) = self.get(base) {
            for f in &contract.functions {
                methods.push(f.name.clone());
            }
            for f in &contract.fields {
                methods.push(f.name.clone());
            }
        }
        // Include methods from satisfied contracts
        if let Some(satisfied) = self.satisfies_map.get(base) {
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
