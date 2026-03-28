use crate::ast::*;
use crate::errors::RuleError;

/// Validate struct implementations match their contract blocks
pub fn check_structs(file: &SourceFile) -> Vec<RuleError> {
    let mut errors = Vec::new();

    for item in &file.items {
        if let Item::Struct(s) = item {
            // Every signature in the contract block must have an implementation
            for sig in &s.signatures {
                let found = s.methods.iter().any(|m| m.name == sig.name);
                if !found {
                    errors.push(RuleError {
                        code: "missing-impl".into(),
                        message: format!(
                            "struct '{}' declares '{}' in contract but has no implementation",
                            s.name, sig.name
                        ),
                        context: None,
                    });
                }
            }

            // Every implementation must match a signature in the contract block
            for method in &s.methods {
                let sig = s.signatures.iter().find(|sig| sig.name == method.name);
                if let Some(sig) = sig {
                    // Check param count matches
                    if sig.params.len() != method.params.len() {
                        errors.push(RuleError {
                            code: "sig-mismatch".into(),
                            message: format!(
                                "'{}.{}' has {} params but contract declares {}",
                                s.name, method.name, method.params.len(), sig.params.len()
                            ),
                            context: None,
                        });
                    }
                    // Check return type matches
                    if sig.return_type != method.return_type {
                        errors.push(RuleError {
                            code: "sig-mismatch".into(),
                            message: format!(
                                "'{}.{}' returns {:?} but contract declares {:?}",
                                s.name, method.name, method.return_type, sig.return_type
                            ),
                            context: None,
                        });
                    }
                } else {
                    errors.push(RuleError {
                        code: "undeclared-method".into(),
                        message: format!(
                            "'{}.{}' is not declared in the struct's contract block",
                            s.name, method.name
                        ),
                        context: None,
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
    fn valid_struct() {
        let file = parse::parse(r#"
            struct Price {
                amount: Number
                add(other: Price) -> Price
            }{
                fn add(other: Price) -> Price {
                    return self
                    test { self(self) == self }
                }
            }
        "#);
        let errors = check_structs(&file);
        assert!(errors.is_empty());
    }

    #[test]
    fn missing_implementation() {
        let file = parse::parse(r#"
            struct Price {
                amount: Number
                add(other: Price) -> Price
                to_string() -> String
            }{
                fn add(other: Price) -> Price {
                    return self
                    test { self(self) == self }
                }
            }
        "#);
        let errors = check_structs(&file);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "missing-impl");
    }
}
