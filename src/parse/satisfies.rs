use crate::ast::*;
use super::expr::Parser;
use super::tokenizer::Token;

impl Parser {
    /// Parse: StructName satisfies ContractName { implementations }
    pub fn parse_satisfies(&mut self, struct_name: String) -> SatisfiesDef {
        self.expect(&Token::Satisfies);
        let contract_name = self.expect_ident();
        self.expect(&Token::LBrace);

        let mut methods = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            methods.push(self.parse_function(false));
        }
        self.expect(&Token::RBrace);

        SatisfiesDef {
            struct_name,
            contract_name,
            methods,
        }
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
        let s = p.parse_satisfies("Email".to_string());
        assert_eq!(s.struct_name, "Email");
        assert_eq!(s.contract_name, "Stringable");
        assert_eq!(s.methods.len(), 1);
        assert_eq!(s.methods[0].name, "to_string");
    }
}
