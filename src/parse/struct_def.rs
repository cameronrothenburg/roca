//! Struct parser — `struct Name { contract }{ impl }`.

use crate::ast::*;
use super::expr::{Parser, ParseResult};
use super::tokenizer::Token;

impl Parser {
    /// Parse: struct Name { contract }{ impl }
    pub fn parse_struct_def(&mut self, is_pub: bool) -> ParseResult<StructDef> {
        self.expect(&Token::Struct)?;
        let name = self.expect_ident()?;

        // First {} — contract block: fields + fn signatures
        self.expect(&Token::LBrace)?;
        let mut fields = Vec::new();
        let mut signatures = Vec::new();

        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            // Doc comments before signatures are consumed by parse_fn_signature.
            // For fields, skip doc comments (fields don't have doc).
            if matches!(self.peek(), Token::DocComment(_)) && !matches!(self.peek_ahead(1), Token::Ident(_)) {
                self.advance();
                continue;
            }

            // Determine if field (name: Type) or fn signature (name(...) -> Type)
            if let Token::Ident(_) = self.peek() {
                if matches!(self.peek_ahead(1), Token::Colon) {
                    // Field
                    let fname = self.expect_ident()?;
                    self.expect(&Token::Colon)?;
                    let type_ref = self.parse_type_ref()?;
                    let constraints = self.parse_constraints()?;
                    fields.push(Field { name: fname, type_ref, constraints });
                } else if matches!(self.peek_ahead(1), Token::LParen) {
                    // Function signature (parse_fn_signature collects doc comments)
                    signatures.push(self.parse_fn_signature()?);
                } else {
                    return Err(self.err(format!("expected field or fn signature in struct contract block, got {:?}", self.peek())));
                }
            } else if matches!(self.peek(), Token::DocComment(_)) {
                // Doc comment before a field/signature — parse_fn_signature will consume it
                signatures.push(self.parse_fn_signature()?);
            } else {
                return Err(self.err(format!("expected identifier in struct contract block, got {:?}", self.peek())));
            }
        }
        self.expect(&Token::RBrace)?;

        // Second {} — implementation block: fn bodies
        self.expect(&Token::LBrace)?;
        let mut methods = Vec::new();

        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            let doc = self.collect_doc();
            let method_pub = self.eat(&Token::Pub);
            let mut method = self.parse_function(method_pub)?;
            if method.doc.is_none() {
                method.doc = doc;
            }
            methods.push(method);
        }
        self.expect(&Token::RBrace)?;

        // Sync visibility from implementations to contract signatures
        for sig in &mut signatures {
            if let Some(method) = methods.iter().find(|m| m.name == sig.name) {
                sig.is_pub = method.is_pub;
            }
        }

        Ok(StructDef {
            name,
            is_pub,
            doc: None,
            fields,
            signatures,
            methods,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::tokenize;

    #[test]
    fn parse_simple_struct() {
        let src = r#"struct Price {
            amount: Number
            add(other: Price) -> Price
            to_string() -> String
        }{
            fn add(other: Price) -> Price {
                return Price { amount: self.amount + other.amount }
                test { self(Price { amount: 5 }) == Price { amount: 15 } }
            }
            fn to_string() -> String {
                return "$"
                test { self() == "$10" }
            }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let s = p.parse_struct_def(true).unwrap();
        assert_eq!(s.name, "Price");
        assert_eq!(s.fields.len(), 1);
        assert_eq!(s.fields[0].name, "amount");
        assert_eq!(s.signatures.len(), 2);
        assert_eq!(s.methods.len(), 2);
    }

    #[test]
    fn parse_struct_with_errors() {
        let src = r#"struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err missing = "value is required"
                err invalid = "format is not valid"
            }
        }{
            fn validate(raw: String) -> Email, err {
                if raw == "" { return err.missing }
                return Email { value: raw }
                test {
                    self("a@b.com") is Ok
                    self("") is err.missing
                }
            }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let s = p.parse_struct_def(false).unwrap();
        assert_eq!(s.signatures[0].errors.len(), 2);
        assert!(s.signatures[0].returns_err);
        assert_eq!(s.methods.len(), 1);
    }
}
