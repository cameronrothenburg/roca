//! Rule: reserved-name
//! Rejects user-defined contracts, structs, or enums that collide with stdlib names.

use crate::ast::*;
use crate::constants::RESERVED_NAMES;
use crate::errors::{self, RuleError};
use crate::check::rule::Rule;
use crate::check::context::ItemContext;

pub struct ReservedNameRule;

impl Rule for ReservedNameRule {
    fn name(&self) -> &'static str { errors::RESERVED_NAME }

    fn check_item(&self, ctx: &ItemContext) -> Vec<RuleError> {
        let mut errs = Vec::new();

        let (kind, name) = match ctx.item {
            Item::Contract(c) => ("contract", &c.name),
            Item::Struct(s) => ("struct", &s.name),
            Item::Enum(e) => ("enum", &e.name),
            // ExternContract is stdlib — not user code
            _ => return errs,
        };

        if RESERVED_NAMES.contains(&name.as_str()) {
            errs.push(RuleError::new(
                errors::RESERVED_NAME,
                format!("`{}` is a reserved stdlib name — user code cannot define a {} with this name", name, kind),
                None,
            ));
        }

        errs
    }
}

#[cfg(test)]
mod tests {
    use crate::check;

    fn errors(src: &str) -> Vec<crate::errors::RuleError> {
        check::check(&crate::parse::parse(src))
    }

    fn has_code(errs: &[crate::errors::RuleError], code: &str) -> bool {
        errs.iter().any(|e| e.code == code)
    }

    #[test]
    fn user_struct_named_math_rejected() {
        let errs = errors("/// Bad\npub struct Math { x: Number }{ }");
        assert!(has_code(&errs, "reserved-name"), "expected reserved-name, got: {:?}", errs);
    }

    #[test]
    fn user_contract_named_json_rejected() {
        let errs = errors("/// Bad\npub contract JSON { parse() -> String }");
        assert!(has_code(&errs, "reserved-name"), "expected reserved-name, got: {:?}", errs);
    }

    #[test]
    fn user_enum_named_crypto_rejected() {
        let errs = errors("/// Bad\npub enum Crypto { Aes = \"aes\" }");
        assert!(has_code(&errs, "reserved-name"), "expected reserved-name, got: {:?}", errs);
    }

    #[test]
    fn user_struct_custom_name_ok() {
        let errs = errors("/// Fine\npub struct MyThing { x: Number }{ }");
        assert!(!has_code(&errs, "reserved-name"));
    }

    #[test]
    fn extern_contract_not_rejected() {
        // Extern contracts are stdlib definitions, not user code
        let errs = errors("/// Stdlib\npub extern contract Math { floor(n: Number) -> Number }");
        assert!(!has_code(&errs, "reserved-name"));
    }
}
