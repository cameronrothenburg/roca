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
    } else {
        panic!("expected ExternFn");
    }
}
