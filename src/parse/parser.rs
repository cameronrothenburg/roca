use crate::ast::*;
use crate::errors::ParseError;
use super::expr::{Parser, ParseResult};
use super::tokenizer::Token;

impl Parser {
    /// Parse a complete source file into a SourceFile AST
    pub fn parse_file(&mut self) -> ParseResult<SourceFile> {
        let mut items = Vec::new();

        while !self.at(&Token::EOF) {
            // Imports before pub/doc check
            if self.at(&Token::Import) {
                items.push(Item::Import(self.parse_import()?));
                continue;
            }

            // Collect doc comments before item
            let doc = self.collect_doc();

            let is_pub = self.eat(&Token::Pub);

            match self.peek().clone() {
                Token::Extern => {
                    self.advance();
                    match self.peek() {
                        Token::Contract => {
                            let mut c = self.parse_contract(is_pub)?;
                            c.doc = doc;
                            items.push(Item::ExternContract(c));
                        }
                        Token::Fn => {
                            let mut f = self.parse_extern_fn()?;
                            f.doc = doc;
                            items.push(Item::ExternFn(f));
                        }
                        _ => return Err(self.err("expected 'contract' or 'fn' after 'extern'")),
                    }
                }
                Token::Contract => {
                    let mut c = self.parse_contract(is_pub)?;
                    c.doc = doc;
                    items.push(Item::Contract(c));
                }
                Token::Struct => {
                    let mut s = self.parse_struct_def(is_pub)?;
                    s.doc = doc;
                    items.push(Item::Struct(s));
                }
                Token::Enum => {
                    let mut e = self.parse_enum(is_pub)?;
                    e.doc = doc;
                    items.push(Item::Enum(e));
                }
                Token::Fn => {
                    let mut f = self.parse_function(is_pub)?;
                    f.doc = doc;
                    items.push(Item::Function(f));
                }
                // Ident followed by satisfies — e.g. "Email satisfies Stringable {"
                Token::Ident(_name) if self.peek_ahead(1) == &Token::Satisfies => {
                    let name = self.expect_ident()?;
                    items.push(Item::Satisfies(self.parse_satisfies(name)?));
                }
                other => {
                    return Err(self.err(format!("unexpected top-level token: {:?}", other)));
                }
            }
        }

        Ok(SourceFile { items })
    }

    /// Parse: extern fn name(params) -> ReturnType, err { err declarations, mock block }
    fn parse_extern_fn(&mut self) -> ParseResult<ExternFnDef> {
        self.expect(&Token::Fn)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen)?;

        let mut return_type = TypeRef::Ok;
        let mut returns_err = false;
        if self.eat(&Token::Arrow) {
            return_type = self.parse_type_ref()?;
            if self.eat(&Token::Comma) {
                self.expect(&Token::Err)?;
                returns_err = true;
            }
        }

        // Optional block with err declarations and/or mock
        let mut errors = Vec::new();
        let mut mock = None;
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
                    errors.push(ErrDecl { name: err_name, message });
                } else if self.at(&Token::Mock) {
                    mock = Some(self.parse_mock_block()?);
                } else {
                    return Err(self.err(format!("expected err or mock in extern fn block, got {:?}", self.peek())));
                }
            }
            self.expect(&Token::RBrace)?;
            if !errors.is_empty() {
                returns_err = true;
            }
        }

        Ok(ExternFnDef {
            name,
            doc: None,
            params,
            return_type,
            returns_err,
            errors,
            mock,
        })
    }

    /// Parse: enum Name { key = value, ... }
    fn parse_enum(&mut self, is_pub: bool) -> ParseResult<EnumDef> {
        self.expect(&Token::Enum)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;

        let mut variants = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::EOF) {
            let vname = self.expect_ident()?;

            let value = if self.eat(&Token::Assign) {
                // Flat enum: key = "value" or key = 42
                match self.advance() {
                    Token::StringLit(s) => EnumValue::String(s),
                    Token::NumberLit(n) => EnumValue::Number(n),
                    other => return Err(self.err(format!("expected string or number for enum value, got {:?}", other))),
                }
            } else if self.eat(&Token::LParen) {
                // Algebraic: Variant(Type, Type)
                let mut types = Vec::new();
                if !self.at(&Token::RParen) {
                    types.push(self.parse_type_ref()?);
                    while self.eat(&Token::Comma) {
                        types.push(self.parse_type_ref()?);
                    }
                }
                self.expect(&Token::RParen)?;
                EnumValue::Data(types)
            } else {
                // Unit variant: just a name
                EnumValue::Unit
            };

            variants.push(EnumVariant { name: vname, value });
            self.eat(&Token::Comma);
        }
        self.expect(&Token::RBrace)?;

        let is_algebraic = variants.iter().any(|v| matches!(v.value, EnumValue::Data(_) | EnumValue::Unit));
        Ok(EnumDef { name, is_pub, doc: None, variants, is_algebraic })
    }

    /// Parse: import { X } from "./path" or import { X } from std or import { X } from std::module
    fn parse_import(&mut self) -> ParseResult<ImportDef> {
        self.expect(&Token::Import)?;
        self.expect(&Token::LBrace)?;
        let mut names = Vec::new();
        if !self.at(&Token::RBrace) {
            names.push(self.expect_ident()?);
            while self.eat(&Token::Comma) {
                if self.at(&Token::RBrace) { break; }
                names.push(self.expect_ident()?);
            }
        }
        self.expect(&Token::RBrace)?;
        self.expect(&Token::From)?;

        let source = if self.at(&Token::Std) {
            self.advance();
            if self.eat(&Token::ColonColon) {
                let module = self.expect_ident()?;
                ImportSource::Std(Some(module))
            } else {
                ImportSource::Std(None)
            }
        } else {
            match self.advance() {
                Token::StringLit(s) => ImportSource::Path(s),
                other => return Err(self.err(format!("expected string path or std after from, got {:?}", other))),
            }
        };

        Ok(ImportDef { names, source })
    }
}

/// Convenience: parse source string to SourceFile — returns Result
pub fn try_parse(source: &str) -> Result<SourceFile, ParseError> {
    let tokens = super::tokenize(source);
    let mut parser = Parser::new(tokens);
    parser.parse_file()
}

/// Convenience: parse source string to SourceFile — panics on error (for tests and backwards compat)
pub fn parse(source: &str) -> SourceFile {
    try_parse(source).unwrap_or_else(|e| panic!("{}", e))
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "parser_tests_extra.rs"]
mod tests_extra;
