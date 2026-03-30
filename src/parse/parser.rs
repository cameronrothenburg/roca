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
            self.expect(&Token::Assign)?;
            let value = match self.advance() {
                Token::StringLit(s) => EnumValue::String(s),
                Token::NumberLit(n) => EnumValue::Number(n),
                other => return Err(self.err(format!("expected string or number for enum value, got {:?}", other))),
            };
            variants.push(EnumVariant { name: vname, value });
            self.eat(&Token::Comma);
        }
        self.expect(&Token::RBrace)?;

        Ok(EnumDef { name, is_pub, doc: None, variants })
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
mod tests {
    use super::*;

    #[test]
    fn parse_contract_and_struct() {
        let src = r#"
            contract Stringable {
                to_string() -> String
            }

            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err missing = "required"
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
            }

            Email satisfies Stringable {
                fn to_string() -> String {
                    return self.value
                    test { self() == "test" }
                }
            }
        "#;
        let file = parse(src);
        assert_eq!(file.items.len(), 3);
        assert!(matches!(file.items[0], Item::Contract(_)));
        assert!(matches!(file.items[1], Item::Struct(_)));
        assert!(matches!(file.items[2], Item::Satisfies(_)));
    }

    #[test]
    fn parse_pub_function() {
        let src = r#"
            pub fn greet(name: String) -> String {
                return "Hello " + name
                crash { name.trim -> halt }
                test { self("cam") == "Hello cam" }
            }
        "#;
        let file = parse(src);
        assert_eq!(file.items.len(), 1);
        if let Item::Function(f) = &file.items[0] {
            assert!(f.is_pub);
            assert_eq!(f.name, "greet");
            assert!(f.crash.is_some());
            assert!(f.test.is_some());
        }
    }

    #[test]
    fn parse_enum_contract() {
        let src = "contract StatusCode { 200 201 400 404 500 }";
        let file = parse(src);
        if let Item::Contract(c) = &file.items[0] {
            assert_eq!(c.values.len(), 5);
        }
    }

    #[test]
    fn parse_contract_with_mock() {
        let src = r#"
            contract Database {
                save(data: String) -> Ok, err {
                    err connection_lost = "connection lost"
                    err timeout = "timed out"
                }
                mock {
                    save -> Ok
                }
            }
        "#;
        let file = parse(src);
        if let Item::Contract(c) = &file.items[0] {
            assert_eq!(c.functions[0].errors.len(), 2);
            assert!(c.mock.is_some());
        }
    }

    #[test]
    fn parse_full_example() {
        let src = r#"
            contract Stringable {
                to_string() -> String
            }

            contract HttpClient {
                get(url: String) -> Response, err {
                    err timeout = "request timed out"
                    err not_found = "404 not found"
                }
                mock {
                    get -> Ok
                }
            }

            pub struct Email {
                value: String
                validate(raw: String) -> Email, err {
                    err missing = "value is required"
                    err invalid = "format is not valid"
                }
            }{
                fn validate(raw: String) -> Email, err {
                    if raw == "" { return err.missing }
                    return Email { value: raw }
                    crash { raw.len -> halt }
                    test {
                        self("a@b.com") is Ok
                        self("") is err.missing
                    }
                }
            }

            Email satisfies Stringable {
                fn to_string() -> String {
                    return self.value
                    test { self() == "a@b.com" }
                }
            }

            pub fn greet(name: String) -> String {
                let trimmed = name.trim()
                return "Hello " + trimmed
                crash { name.trim -> halt }
                test {
                    self("cam") == "Hello cam"
                }
            }
        "#;
        let file = parse(src);
        assert_eq!(file.items.len(), 5);
        // contract Stringable
        assert!(matches!(&file.items[0], Item::Contract(c) if c.name == "Stringable"));
        // contract HttpClient
        assert!(matches!(&file.items[1], Item::Contract(c) if c.name == "HttpClient"));
        // struct Email
        assert!(matches!(&file.items[2], Item::Struct(s) if s.name == "Email"));
        // Email satisfies Stringable
        assert!(matches!(&file.items[3], Item::Satisfies(s) if s.struct_name == "Email"));
        // fn greet
        assert!(matches!(&file.items[4], Item::Function(f) if f.name == "greet"));
    }

    #[test]
    fn try_parse_returns_error() {
        let result = try_parse("contract {}");
        assert!(result.is_err());
    }

    #[test]
    fn try_parse_bad_syntax() {
        let result = try_parse("fn {}");
        assert!(result.is_err());
    }

    #[test]
    fn parse_extern_contract() {
        let src = r#"
            extern contract NativeResponse {
                status: Number
                ok: Bool
                text() -> String
            }
        "#;
        let file = parse(src);
        assert_eq!(file.items.len(), 1);
        if let Item::ExternContract(c) = &file.items[0] {
            assert_eq!(c.name, "NativeResponse");
            assert_eq!(c.fields.len(), 2);
            assert_eq!(c.functions.len(), 1);
        } else {
            panic!("expected ExternContract");
        }
    }

    #[test]
    fn parse_extern_fn() {
        let src = r#"
            extern fn fetchData(url: String) -> NativeResponse, err {
                err network = "network error"
                err timeout = "request timed out"
            }
        "#;
        let file = parse(src);
        assert_eq!(file.items.len(), 1);
        if let Item::ExternFn(f) = &file.items[0] {
            assert_eq!(f.name, "fetchData");
            assert_eq!(f.params.len(), 1);
            assert!(f.returns_err);
            assert_eq!(f.errors.len(), 2);
        } else {
            panic!("expected ExternFn");
        }
    }

    #[test]
    fn parse_extern_fn_no_errors() {
        let src = "extern fn log(msg: String) -> Ok";
        let file = parse(src);
        if let Item::ExternFn(f) = &file.items[0] {
            assert_eq!(f.name, "log");
            assert!(!f.returns_err);
            assert!(f.errors.is_empty());
            assert!(f.mock.is_none());
        } else {
            panic!("expected ExternFn");
        }
    }

    #[test]
    fn parse_extern_fn_with_mock() {
        let src = r#"
            extern fn globalFetch(url: String) -> NativeResponse, err {
                err network = "network error"
                mock {
                    globalFetch -> NativeResponse { status: 200, body: "ok" }
                }
            }
        "#;
        let file = parse(src);
        if let Item::ExternFn(f) = &file.items[0] {
            assert_eq!(f.name, "globalFetch");
            assert!(f.returns_err);
            assert_eq!(f.errors.len(), 1);
            assert!(f.mock.is_some());
            assert_eq!(f.mock.as_ref().unwrap().entries.len(), 1);
        } else {
            panic!("expected ExternFn");
        }
    }

    #[test]
    fn unterminated_contract() {
        let result = try_parse("contract Foo {");
        assert!(result.is_err(), "unterminated contract should fail");
    }

    #[test]
    fn empty_function_body() {
        let file = parse("fn foo() -> Number {}");
        assert_eq!(file.items.len(), 1);
        if let Item::Function(f) = &file.items[0] {
            assert_eq!(f.name, "foo");
            assert_eq!(f.return_type, TypeRef::Number);
            assert!(f.body.is_empty());
            assert!(f.crash.is_none());
            assert!(f.test.is_none());
        } else {
            panic!("expected Function");
        }
    }

    #[test]
    fn duplicate_field_names_in_struct() {
        let src = r#"struct S {
            name: String
            name: Number
        }{}"#;
        let file = parse(src);
        assert_eq!(file.items.len(), 1);
        if let Item::Struct(s) = &file.items[0] {
            assert_eq!(s.fields.len(), 2);
            assert_eq!(s.fields[0].name, "name");
            assert_eq!(s.fields[1].name, "name");
            assert_eq!(s.fields[0].type_ref, TypeRef::String);
            assert_eq!(s.fields[1].type_ref, TypeRef::Number);
        } else {
            panic!("expected Struct");
        }
    }

    #[test]
    fn nested_generics() {
        let src = r#"contract Foo<T> {
            get() -> Array<T>
        }"#;
        let file = parse(src);
        if let Item::Contract(c) = &file.items[0] {
            assert_eq!(c.name, "Foo");
            assert_eq!(c.type_params.len(), 1);
            assert_eq!(c.type_params[0].name, "T");
            assert_eq!(c.type_params[0].constraint, None);
            assert_eq!(c.functions.len(), 1);
            assert!(
                matches!(&c.functions[0].return_type, TypeRef::Generic(name, args) if name == "Array" && args.len() == 1),
                "expected Array<T> return type, got {:?}", c.functions[0].return_type
            );
        } else {
            panic!("expected Contract");
        }
    }

    #[test]
    fn extern_fn_with_no_block() {
        let file = parse("extern fn log(msg: String) -> Ok");
        assert_eq!(file.items.len(), 1);
        if let Item::ExternFn(f) = &file.items[0] {
            assert_eq!(f.name, "log");
            assert_eq!(f.params.len(), 1);
            assert_eq!(f.return_type, TypeRef::Ok);
            assert!(!f.returns_err);
            assert!(f.errors.is_empty());
            assert!(f.mock.is_none());
        } else {
            panic!("expected ExternFn");
        }
    }

    #[test]
    fn multiple_satisfies() {
        let src = r#"
            Email satisfies Stringable {
                fn to_string() -> String {
                    return self.value
                    test { self() == "test" }
                }
            }

            Email satisfies Loggable {
                fn log() -> Ok {
                    return Ok
                    test { self() is Ok }
                }
            }
        "#;
        let file = parse(src);
        assert_eq!(file.items.len(), 2);
        if let Item::Satisfies(s) = &file.items[0] {
            assert_eq!(s.struct_name, "Email");
            assert_eq!(s.contract_name, "Stringable");
        } else {
            panic!("expected Satisfies for first item");
        }
        if let Item::Satisfies(s) = &file.items[1] {
            assert_eq!(s.struct_name, "Email");
            assert_eq!(s.contract_name, "Loggable");
        } else {
            panic!("expected Satisfies for second item");
        }
    }

    #[test]
    fn match_with_no_default_arm() {
        let src = r#"
            fn classify(x: Number) -> String {
                return match x {
                    1 => "a"
                    2 => "b"
                }
                test { self(1) == "a" }
            }
        "#;
        let file = parse(src);
        if let Item::Function(f) = &file.items[0] {
            assert_eq!(f.name, "classify");
            // The match parsed without a default arm — just verify it parsed at all
            assert!(!f.body.is_empty());
        } else {
            panic!("expected Function");
        }
    }

    #[test]
    fn parse_error_gives_useful_position() {
        let result = try_parse("fn { }");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.pos > 0, "error position should be non-zero");
        assert!(
            err.message.contains("identifier"),
            "error message should mention 'identifier', got: {}",
            err.message
        );
    }

    #[test]
    fn contract_with_generic_constraint() {
        let src = r#"contract Logger<T: Loggable> {
            add(item: T) -> Number
        }"#;
        let file = parse(src);
        if let Item::Contract(c) = &file.items[0] {
            assert_eq!(c.name, "Logger");
            assert_eq!(c.type_params.len(), 1);
            assert_eq!(c.type_params[0].name, "T");
            assert_eq!(c.type_params[0].constraint, Some("Loggable".to_string()));
            assert_eq!(c.functions.len(), 1);
            assert_eq!(c.functions[0].name, "add");
        } else {
            panic!("expected Contract");
        }
    }

    #[test]
    fn enum_with_mixed_value_types() {
        let src = r#"enum Mixed {
            a = "str",
            b = 42
        }"#;
        // The parser accepts both string and number enum values — mixed types are allowed at parse time
        let file = parse(src);
        if let Item::Enum(e) = &file.items[0] {
            assert_eq!(e.name, "Mixed");
            assert_eq!(e.variants.len(), 2);
            assert!(matches!(&e.variants[0].value, EnumValue::String(s) if s == "str"));
            assert!(matches!(&e.variants[1].value, EnumValue::Number(n) if *n == 42.0));
        } else {
            panic!("expected Enum");
        }
    }
}
