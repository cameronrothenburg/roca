//! Satisfies parser — `StructName satisfies ContractName { methods }`.

use crate::ast::*;
use super::expr::{Parser, ParseResult};
use super::tokenizer::Token;

impl Parser {
    /// Parse: StructName satisfies ContractName { implementations }
    pub fn parse_satisfies(&mut self, struct_name: String) -> ParseResult<SatisfiesDef> {
        self.expect(&Token::Satisfies)?;
        let contract_name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;

        let mut methods = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            let doc = self.collect_doc();
            let mut method = self.parse_function(false)?;
            if method.doc.is_none() {
                method.doc = doc;
            }
            methods.push(method);
        }
        self.expect(&Token::RBrace)?;

        Ok(SatisfiesDef {
            struct_name,
            contract_name,
            methods,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::tokenize;

    #[test]
    fn parse_satisfies_block() {
        let src = r#"satisfies Stringable {
            fn to_string() -> String {
                return self.value
                test { self() == "test" }
            }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let s = p.parse_satisfies("Email".to_string()).unwrap();
        assert_eq!(s.struct_name, "Email");
        assert_eq!(s.contract_name, "Stringable");
        assert_eq!(s.methods.len(), 1);
        assert_eq!(s.methods[0].name, "to_string");
    }

    #[test]
    fn parse_empty_satisfies() {
        let src = "satisfies Loggable {}";
        let mut p = Parser::new(tokenize(src));
        let s = p.parse_satisfies("String".to_string()).unwrap();
        assert_eq!(s.struct_name, "String");
        assert_eq!(s.contract_name, "Loggable");
        assert_eq!(s.methods.len(), 0);
    }
}
