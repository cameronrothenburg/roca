//! Rule: unknown-contract, missing-satisfies, satisfies-mismatch
//! Validates satisfies declarations against their target contracts.

use roca_ast::*;
use roca_errors as errors;
use roca_errors::RuleError;
use crate::rule::Rule;
use crate::context::ItemContext;

#[cfg(test)]
mod tests {
    

    fn errors(src: &str) -> Vec<roca_errors::RuleError> {
        crate::check(&roca_parse::parse(src))
    }

    #[test]
    fn valid_satisfies() {
        let e = errors(r#"contract Stringable { to_string() -> String } pub struct E { value: String }{} E satisfies Stringable { fn to_string() -> String { return self.value test { self() == "t" } } }"#);
        assert!(!e.iter().any(|e| e.code == "missing-satisfies" || e.code == "unknown-contract"));
    }

    #[test]
    fn missing_method() {
        let e = errors(r#"contract S { serialize() -> String deserialize(r: String) -> String } pub struct E { value: String }{} E satisfies S { fn serialize() -> String { return self.value test { self() == "t" } } }"#);
        assert!(e.iter().any(|e| e.code == "missing-satisfies"));
    }

    #[test]
    fn unknown_contract() {
        let e = errors(r#"pub struct E { value: String }{} E satisfies Nonexistent { fn foo() -> String { return "bar" test { self() == "bar" } } }"#);
        assert!(e.iter().any(|e| e.code == "unknown-contract"));
    }

    #[test]
    fn satisfies_param_count_mismatch() {
        // Contract requires serialize() with 0 params, but impl has 1 param
        let e = errors(r#"contract S { serialize() -> String } pub struct E { value: String }{} E satisfies S { fn serialize(extra: String) -> String { return extra test { self("x") == "x" } } }"#);
        assert!(e.iter().any(|e| e.code == "satisfies-mismatch"));
    }

    #[test]
    fn satisfies_return_type_mismatch() {
        // Contract requires -> String but impl returns -> Number
        let e = errors(r#"contract S { serialize() -> String } pub struct E { value: String }{} E satisfies S { fn serialize() -> Number { return 42 test { self() == 42 } } }"#);
        assert!(e.iter().any(|e| e.code == "satisfies-mismatch"));
    }

    #[test]
    fn valid_satisfies_multiple_methods() {
        let e = errors(r#"contract S { serialize() -> String deserialize(r: String) -> String } pub struct E { value: String }{} E satisfies S { fn serialize() -> String { return self.value test { self() == "t" } } fn deserialize(r: String) -> String { return r test { self("x") == "x" } } }"#);
        assert!(!e.iter().any(|e| e.code == "missing-satisfies" || e.code == "satisfies-mismatch" || e.code == "unknown-contract"));
    }
}

/// Substitute generic type params in a type reference.
/// e.g. with map {T → User}, TypeRef::Named("T") becomes TypeRef::Named("User")
fn substitute_type(ty: &roca_ast::TypeRef, map: &std::collections::HashMap<String, &roca_ast::TypeRef>) -> roca_ast::TypeRef {
    use roca_ast::TypeRef;
    match ty {
        TypeRef::Named(name) => {
            if let Some(replacement) = map.get(name) {
                (*replacement).clone()
            } else {
                ty.clone()
            }
        }
        TypeRef::Generic(name, args) => {
            let subst_args: Vec<TypeRef> = args.iter().map(|a| substitute_type(a, map)).collect();
            TypeRef::Generic(name.clone(), subst_args)
        }
        TypeRef::Nullable(inner) => {
            TypeRef::Nullable(Box::new(substitute_type(inner, map)))
        }
        _ => ty.clone(),
    }
}

pub struct SatisfiesRule;

impl Rule for SatisfiesRule {
    fn name(&self) -> &'static str { "satisfies" }

    fn check_item(&self, ctx: &ItemContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        if let Item::Satisfies(sat) = ctx.item {
            let contract = match ctx.check.registry.contracts.get(&sat.contract_name) {
                Some(c) => c,
                None => {
                    errors.push(RuleError::new(errors::UNKNOWN_CONTRACT, format!("'{}' satisfies '{}' but contract '{}' does not exist", sat.struct_name, sat.contract_name, sat.contract_name), None));
                    return errors;
                }
            };
            // Build type param → type arg substitution map
            // e.g. Deserializable<User> with contract<T> → T maps to User
            let type_map: std::collections::HashMap<String, &roca_ast::TypeRef> = contract.type_params.iter()
                .zip(sat.type_args.iter())
                .map(|(param, arg)| (param.name.clone(), arg))
                .collect();

            for sig in &contract.functions {
                match sat.methods.iter().find(|m| m.name == sig.name) {
                    None => {
                        errors.push(RuleError::new(errors::MISSING_SATISFIES, format!("'{}' satisfies '{}' but does not implement '{}'", sat.struct_name, sat.contract_name, sig.name), None));
                    }
                    Some(m) => {
                        if sig.params.len() != m.params.len() {
                            errors.push(RuleError::new(errors::SATISFIES_MISMATCH, format!("'{}.{}' has {} params but '{}' requires {}", sat.struct_name, m.name, m.params.len(), sat.contract_name, sig.params.len()), None));
                        }
                        // Substitute generic type params before comparing return types
                        let expected_return = substitute_type(&sig.return_type, &type_map);
                        if expected_return != m.return_type {
                            errors.push(RuleError::new(errors::SATISFIES_MISMATCH, format!("'{}.{}' returns {:?} but '{}' requires {:?}", sat.struct_name, m.name, m.return_type, sat.contract_name, expected_return), None));
                        }
                    }
                }
            }
        }
        errors
    }
}
