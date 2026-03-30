use crate::ast::*;
use super::expr::{Parser, ParseResult};
use super::tokenizer::Token;

impl Parser {
    /// Parse: contract Name { signatures, errors, mock }
    pub fn parse_contract(&mut self, is_pub: bool) -> ParseResult<ContractDef> {
        self.expect(&Token::Contract)?;
        let name = self.expect_ident()?;

        // Parse optional type params: <T, V: Constraint>
        let type_params = if self.at(&Token::Lt) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        self.expect(&Token::LBrace)?;

        let mut functions = Vec::new();
        let mut fields = Vec::new();
        let mut mock = None;
        let mut values = Vec::new();

        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            match self.peek() {
                // Mock block
                Token::Mock => {
                    mock = Some(self.parse_mock_block()?);
                }
                // Error declaration on contract-level (shouldn't happen at top level)
                Token::Err => {
                    return Err(self.err("err declarations must be inside function signatures"));
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
                // Doc comment — consume it, then parse the next thing (field or signature)
                Token::DocComment(_) => {
                    // Don't consume here — parse_fn_signature calls collect_doc()
                    // But if next ident is a field (name: Type), skip the doc and parse the field
                    // Peek past doc comments to see what follows
                    let mut lookahead = 0;
                    while matches!(self.peek_ahead(lookahead), Token::DocComment(_)) {
                        lookahead += 1;
                    }
                    if matches!(self.peek_ahead(lookahead + 1), Token::Colon) {
                        // Field — skip doc comments, parse field
                        self.collect_doc();
                        let fname = self.expect_ident()?;
                        self.expect(&Token::Colon)?;
                        let type_ref = self.parse_type_ref()?;
                        let constraints = self.parse_constraints()?;
                        fields.push(Field { name: fname, type_ref, constraints });
                    } else {
                        // Function signature (parse_fn_signature collects doc)
                        functions.push(self.parse_fn_signature()?);
                    }
                }
                // Identifier — could be a field or function signature
                Token::Ident(_) => {
                    if matches!(self.peek_ahead(1), Token::Colon) {
                        let fname = self.expect_ident()?;
                        self.expect(&Token::Colon)?;
                        let type_ref = self.parse_type_ref()?;
                        let constraints = self.parse_constraints()?;
                        fields.push(Field { name: fname, type_ref, constraints });
                    } else {
                        functions.push(self.parse_fn_signature()?);
                    }
                }
                _ => {
                    return Err(self.err(format!("unexpected token in contract: {:?}", self.peek())));
                }
            }
        }
        self.expect(&Token::RBrace)?;

        Ok(ContractDef {
            name,
            is_pub,
            doc: None,
            type_params,
            functions,
            fields,
            mock,
            values,
        })
    }

    /// Parse a function signature (no body): name(params) -> ReturnType { err declarations }
    pub fn parse_fn_signature(&mut self) -> ParseResult<FnSignature> {
        let doc = self.collect_doc();
        let name = self.expect_ident()?;
        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen)?;

        // Return type
        let mut return_type = TypeRef::Ok;
        let mut returns_err = false;
        if self.eat(&Token::Arrow) {
            return_type = self.parse_type_ref()?;
            // Check for , err
            if self.eat(&Token::Comma) {
                self.expect(&Token::Err)?;
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
                    let err_name = self.expect_ident()?;
                    self.expect(&Token::Assign)?;
                    let message = match self.advance() {
                        Token::StringLit(s) => s,
                        other => return Err(self.err(format!("expected string for error message, got {:?}", other))),
                    };
                    errors.push(ErrDecl {
                        name: err_name,
                        message,
                    });
                } else {
                    return Err(self.err(format!("expected err declaration in signature block, got {:?}", self.peek())));
                }
            }
            self.expect(&Token::RBrace)?;
            if !errors.is_empty() {
                returns_err = true;
            }
        }

        Ok(FnSignature {
            name,
            is_pub: true,
            doc,
            params,
            return_type,
            returns_err,
            errors,
        })
    }

    /// Parse parameter list: (name: Type, name: Type)
    pub fn parse_params(&mut self) -> ParseResult<Vec<Param>> {
        let mut params = Vec::new();
        if !self.at(&Token::RParen) {
            let name = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let type_ref = self.parse_type_ref()?;
            params.push(Param { name, type_ref });

            while self.eat(&Token::Comma) {
                let name = self.expect_ident()?;
                self.expect(&Token::Colon)?;
                let type_ref = self.parse_type_ref()?;
                params.push(Param { name, type_ref });
            }
        }
        Ok(params)
    }

    /// Parse a type reference: String, Number, Bool, Named, Self, or Type | null
    pub fn parse_type_ref(&mut self) -> ParseResult<TypeRef> {
        let base = match self.advance() {
            Token::Ident(s) => {
                let name = s.clone();
                // Check for generic: Type<T, U>
                if self.at(&Token::Lt) {
                    self.advance();
                    let mut type_args = vec![self.parse_type_ref()?];
                    while self.eat(&Token::Comma) {
                        type_args.push(self.parse_type_ref()?);
                    }
                    self.expect(&Token::Gt)?;
                    TypeRef::Generic(name, type_args)
                } else {
                    TypeRef::from_str(&name)
                }
            }
            Token::SelfKw => TypeRef::Named("Self".to_string()),
            Token::Ok => TypeRef::Ok,
            other => return Err(self.err(format!("expected type, got {:?}", other))),
        };

        // Check for | null
        if self.at(&Token::Pipe) {
            self.advance();
            self.expect(&Token::Null)?;
            return Ok(TypeRef::Nullable(Box::new(base)));
        }

        Ok(base)
    }

    /// Parse type parameters: <T, V: Constraint>
    pub fn parse_type_params(&mut self) -> ParseResult<Vec<TypeParam>> {
        self.expect(&Token::Lt)?;
        let mut params = Vec::new();

        loop {
            let name = self.expect_ident()?;
            let constraint = if self.eat(&Token::Colon) {
                Some(self.expect_ident()?)
            } else {
                None
            };
            params.push(TypeParam { name, constraint });
            if !self.eat(&Token::Comma) { break; }
        }

        self.expect(&Token::Gt)?;
        Ok(params)
    }

    /// Parse optional field constraints: { min: 0, max: 255, contains: "@" }
    /// Returns empty vec if no constraints block present.
    pub fn parse_constraints(&mut self) -> ParseResult<Vec<Constraint>> {
        // Constraints start with { but only if the next tokens look like key: value, not a block body
        // Disambiguate: { ident : value } is constraints, { stmt } is a block
        if !self.at(&Token::LBrace) {
            return Ok(Vec::new());
        }

        // Peek: if next is an ident or 'default' keyword followed by colon, it's constraints
        let is_constraint_key = matches!(self.peek_ahead(1), Token::Ident(_) | Token::Default);
        if !is_constraint_key || !matches!(self.peek_ahead(2), Token::Colon) {
            return Ok(Vec::new());
        }

        self.advance(); // consume {
        let mut constraints = Vec::new();

        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            // Handle 'default' keyword specially — it's a keyword token, not an ident
            if self.at(&Token::Default) {
                self.advance();
                self.expect(&Token::Colon)?;
                match self.advance() {
                    Token::StringLit(s) => constraints.push(Constraint::Default(s)),
                    Token::NumberLit(n) => constraints.push(Constraint::Default(format!("{}", n))),
                    Token::BoolLit(b) => constraints.push(Constraint::Default(format!("{}", b))),
                    other => return Err(self.err(format!("expected default value, got {:?}", other))),
                }
                self.eat(&Token::Comma);
                continue;
            }

            let key = self.expect_ident()?;
            self.expect(&Token::Colon)?;

            match key.as_str() {
                "min" => {
                    if let Token::NumberLit(n) = self.advance() {
                        constraints.push(Constraint::Min(n));
                    }
                }
                "max" => {
                    if let Token::NumberLit(n) = self.advance() {
                        constraints.push(Constraint::Max(n));
                    }
                }
                "minLen" => {
                    if let Token::NumberLit(n) = self.advance() {
                        constraints.push(Constraint::MinLen(n));
                    }
                }
                "maxLen" => {
                    if let Token::NumberLit(n) = self.advance() {
                        constraints.push(Constraint::MaxLen(n));
                    }
                }
                "contains" => {
                    if let Token::StringLit(s) = self.advance() {
                        constraints.push(Constraint::Contains(s));
                    }
                }
                "pattern" => {
                    if let Token::StringLit(s) = self.advance() {
                        constraints.push(Constraint::Pattern(s));
                    }
                }
                other => return Err(self.err(format!("unknown constraint: {}", other))),
            }

            self.eat(&Token::Comma);
        }
        self.expect(&Token::RBrace)?;

        Ok(constraints)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::tokenize;

    #[test]
    fn parse_simple_contract() {
        let mut p = Parser::new(tokenize("contract Stringable { to_string() -> String }"));
        let c = p.parse_contract(false).unwrap();
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
        let c = p.parse_contract(false).unwrap();
        assert_eq!(c.name, "HttpClient");
        assert_eq!(c.functions[0].errors.len(), 2);
        assert_eq!(c.functions[0].errors[0].name, "timeout");
        assert!(c.functions[0].returns_err);
    }

    #[test]
    fn parse_contract_with_fields() {
        let src = "contract Response { status: StatusCode body: String }";
        let mut p = Parser::new(tokenize(src));
        let c = p.parse_contract(false).unwrap();
        assert_eq!(c.fields.len(), 2);
        assert_eq!(c.fields[0].name, "status");
    }

    #[test]
    fn parse_enum_contract() {
        let src = "contract StatusCode { 200 201 400 404 500 }";
        let mut p = Parser::new(tokenize(src));
        let c = p.parse_contract(false).unwrap();
        assert_eq!(c.values.len(), 5);
    }
}
