use crate::ast::{MockDef, MockEntry};
use super::expr::Parser;
use super::tokenizer::Token;

impl Parser {
    /// Parse: mock { method -> value, ... }
    pub fn parse_mock_block(&mut self) -> MockDef {
        self.expect(&Token::Mock);
        self.expect(&Token::LBrace);

        let mut entries = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            let method = self.expect_ident();
            self.expect(&Token::Arrow);
            let value = self.parse_expr();
            entries.push(MockEntry { method, value });
        }
        self.expect(&Token::RBrace);

        MockDef { entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;
    use crate::parse::tokenize;

    #[test]
    fn parse_mock_block() {
        let mut p = Parser::new(tokenize("mock { save -> Ok read -> \"content\" }"));
        let m = p.parse_mock_block();
        assert_eq!(m.entries.len(), 2);
        assert_eq!(m.entries[0].method, "save");
        assert_eq!(m.entries[1].method, "read");
        assert_eq!(m.entries[1].value, Expr::String("content".into()));
    }
}
