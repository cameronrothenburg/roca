use crate::ast::*;
use super::expr::Parser;
use super::tokenizer::Token;

impl Parser {
    /// Parse: contract Name { signatures, errors, mock }
    pub fn parse_contract(&mut self, is_pub: bool) -> ContractDef {
        self.expect(&Token::Contract);
        let name = self.expect_ident();
        self.expect(&Token::LBrace);

        let mut functions = Vec::new();
        let mut fields = Vec::new();
        let mut mock = None;
        let mut values = Vec::new();

        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            match self.peek() {
                // Mock block
                Token::Mock => {
                    mock = Some(self.parse_mock_block());
                }
                // Error declaration on contract-level (shouldn't happen at top level)
                Token::Err => {
                    // Skip — errors are inside function signatures
                    panic!("err declarations must be inside function signatures");
                }
                // Number literal — enum value
                Token::NumberLit(_) => {
                    if let Token::NumberLit(n) = self.advance() {
                        values.push(ContractValue::Number(n));
                    }
                }
                // String literal — enum value
                Token::StringLit(_) => {
                    if let Token::StringLit(s) = self.advance() {
                        values.push(ContractValue::String(s));
                    }
                }
                // Identifier — could be a field or function signature
                Token::Ident(_) => {
                    // Peek ahead to determine if field (name: Type) or fn sig (name(...) -> Type)
                    if matches!(self.peek_ahead(1), Token::Colon) {
                        // Field: name: Type
                        let fname = self.expect_ident();
                        self.expect(&Token::Colon);
                        let type_ref = self.parse_type_ref();
                        fields.push(Field {
                            name: fname,
                            type_ref,
                        });
                    } else {
                        // Function signature
                        functions.push(self.parse_fn_signature());
                    }
                }
                _ => {
                    panic!("unexpected token in contract: {:?}", self.peek());
                }
            }
        }
        self.expect(&Token::RBrace);

        ContractDef {
            name,
            is_pub,
            functions,
            fields,
            mock,
            values,
        }
    }

    /// Parse a function signature (no body): name(params) -> ReturnType { err declarations }
    pub fn parse_fn_signature(&mut self) -> FnSignature {
        let name = self.expect_ident();
        self.expect(&Token::LParen);
        let params = self.parse_params();
        self.expect(&Token::RParen);

        // Return type
        let mut return_type = TypeRef::Ok;
        let mut returns_err = false;
        if self.eat(&Token::Arrow) {
            return_type = self.parse_type_ref();
            // Check for , err
            if self.eat(&Token::Comma) {
                self.expect(&Token::Err);
                returns_err = true;
            }
        }

        // Optional error declarations block
        let mut errors = Vec::new();
        if self.at(&Token::LBrace) {
            self.advance();
            while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
                if self.at(&Token::Err) {
                    self.advance();
                    let err_name = self.expect_ident();
                    self.expect(&Token::Assign);
                    let message = match self.advance() {
                        Token::StringLit(s) => s,
                        other => panic!("expected string for error message, got {:?}", other),
                    };
                    errors.push(ErrDecl {
                        name: err_name,
                        message,
                    });
                } else {
                    panic!("expected err declaration in signature block, got {:?}", self.peek());
                }
            }
            self.expect(&Token::RBrace);
            if !errors.is_empty() {
                returns_err = true;
            }
        }

        FnSignature {
            name,
            params,
            return_type,
            returns_err,
            errors,
        }
    }

    /// Parse parameter list: (name: Type, name: Type)
    pub fn parse_params(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        if !self.at(&Token::RParen) {
            let name = self.expect_ident();
            self.expect(&Token::Colon);
            let type_ref = self.parse_type_ref();
            params.push(Param { name, type_ref });

            while self.eat(&Token::Comma) {
                let name = self.expect_ident();
                self.expect(&Token::Colon);
                let type_ref = self.parse_type_ref();
                params.push(Param { name, type_ref });
            }
        }
        params
    }

    /// Parse a type reference: String, Number, Bool, Named, or Self
    pub fn parse_type_ref(&mut self) -> TypeRef {
        match self.advance() {
            Token::Ident(s) => TypeRef::from_str(&s),
            Token::SelfKw => TypeRef::Named("Self".to_string()),
            Token::Ok => TypeRef::Ok,
            other => panic!("expected type, got {:?}", other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::tokenize;

    #[test]
    fn parse_simple_contract() {
        let mut p = Parser::new(tokenize("contract Stringable { to_string() -> String }"));
        let c = p.parse_contract(false);
        assert_eq!(c.name, "Stringable");
        assert_eq!(c.functions.len(), 1);
        assert_eq!(c.functions[0].name, "to_string");
        assert_eq!(c.functions[0].return_type, TypeRef::String);
    }

    #[test]
    fn parse_contract_with_errors() {
        let src = r#"contract HttpClient {
            get(url: String) -> Response, err {
                err timeout = "request timed out"
                err not_found = "404 not found"
            }
        }"#;
        let mut p = Parser::new(tokenize(src));
        let c = p.parse_contract(false);
        assert_eq!(c.name, "HttpClient");
        assert_eq!(c.functions[0].errors.len(), 2);
        assert_eq!(c.functions[0].errors[0].name, "timeout");
        assert!(c.functions[0].returns_err);
    }

    #[test]
    fn parse_contract_with_fields() {
        let src = "contract Response { status: StatusCode body: String }";
        let mut p = Parser::new(tokenize(src));
        let c = p.parse_contract(false);
        assert_eq!(c.fields.len(), 2);
        assert_eq!(c.fields[0].name, "status");
    }

    #[test]
    fn parse_enum_contract() {
        let src = "contract StatusCode { 200 201 400 404 500 }";
        let mut p = Parser::new(tokenize(src));
        let c = p.parse_contract(false);
        assert_eq!(c.values.len(), 5);
    }
}
