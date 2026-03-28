use crate::ast::*;
use super::expr::Parser;
use super::tokenizer::Token;

impl Parser {
    /// Parse a complete source file into a SourceFile AST
    pub fn parse_file(&mut self) -> SourceFile {
        let mut items = Vec::new();

        while !self.at(&Token::EOF) {
            let is_pub = self.eat(&Token::Pub);

            match self.peek().clone() {
                Token::Contract => {
                    items.push(Item::Contract(self.parse_contract(is_pub)));
                }
                Token::Struct => {
                    items.push(Item::Struct(self.parse_struct_def(is_pub)));
                }
                Token::Fn => {
                    items.push(Item::Function(self.parse_function(is_pub)));
                }
                // Ident followed by satisfies — e.g. "Email satisfies Stringable {"
                Token::Ident(name) if self.peek_ahead(1) == &Token::Satisfies => {
                    let name = self.expect_ident();
                    items.push(Item::Satisfies(self.parse_satisfies(name)));
                }
                other => {
                    panic!("unexpected top-level token: {:?}", other);
                }
            }
        }

        SourceFile { items }
    }
}

/// Convenience: parse source string to SourceFile
pub fn parse(source: &str) -> SourceFile {
    let tokens = super::tokenize(source);
    let mut parser = Parser::new(tokens);
    parser.parse_file()
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
}
