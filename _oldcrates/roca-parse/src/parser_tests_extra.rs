use super::*;

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

#[test]
fn parse_algebraic_enum() {
    let file = parse(r#"
        enum Token {
            Number(Number)
            Str(String)
            Plus
            Minus
        }
    "#);
    assert_eq!(file.items.len(), 1);
    if let Item::Enum(e) = &file.items[0] {
        assert_eq!(e.name, "Token");
        assert!(e.is_algebraic);
        assert_eq!(e.variants.len(), 4);
        assert!(matches!(&e.variants[0].value, EnumValue::Data(t) if t.len() == 1));
        assert!(matches!(&e.variants[2].value, EnumValue::Unit));
    } else {
        panic!("expected Enum");
    }
}

#[test]
fn parse_algebraic_enum_multi_field() {
    let file = parse(r#"
        enum Expr {
            BinOp(String, Number, Number)
            Literal(Number)
            Empty
        }
    "#);
    if let Item::Enum(e) = &file.items[0] {
        assert!(e.is_algebraic);
        if let EnumValue::Data(types) = &e.variants[0].value {
            assert_eq!(types.len(), 3);
        } else { panic!("expected Data variant"); }
    } else { panic!("expected Enum"); }
}

#[test]
fn parse_match_variant_pattern() {
    let file = parse(r#"
        pub fn test_match(tag: String) -> String {
            return match tag {
                Token.Number(n) => "num"
                Token.Plus => "plus"
                _ => "other"
            }
        }
    "#);
    if let Item::Function(f) = &file.items[0] {
        if let Stmt::Return(Expr::Match { arms, .. }) = &f.body[0] {
            assert_eq!(arms.len(), 3);
            assert!(matches!(&arms[0].pattern, Some(MatchPattern::Variant { variant, bindings, .. }) if variant == "Number" && bindings.len() == 1));
            assert!(matches!(&arms[1].pattern, Some(MatchPattern::Variant { variant, bindings, .. }) if variant == "Plus" && bindings.is_empty()));
            assert!(arms[2].pattern.is_none());
        } else { panic!("expected match"); }
    } else { panic!("expected function"); }
}

#[test]
fn parse_generic_function() {
    let file = parse(r#"
        pub fn identity<T>(value: T) -> T {
            return value
        test { self(42) == 42 }}
    "#);
    if let Item::Function(f) = &file.items[0] {
        assert_eq!(f.name, "identity");
        assert_eq!(f.type_params.len(), 1);
        assert_eq!(f.type_params[0].name, "T");
        assert_eq!(f.type_params[0].constraint, None);
    } else { panic!("expected function"); }
}

#[test]
fn parse_generic_function_multi_params() {
    let file = parse(r#"
        pub fn pair<A, B>(first: A, second: B) -> Array<A> {
            return [first]
        test { self(1, "x") == [1] }}
    "#);
    if let Item::Function(f) = &file.items[0] {
        assert_eq!(f.type_params.len(), 2);
        assert_eq!(f.type_params[0].name, "A");
        assert_eq!(f.type_params[1].name, "B");
    } else { panic!("expected function"); }
}

#[test]
fn parse_generic_with_constraint() {
    let file = parse(r#"
        pub fn log_item<T: Loggable>(item: T) -> Ok {
            log(item.toLog())
        test { self("hello") is Ok }}
    "#);
    if let Item::Function(f) = &file.items[0] {
        assert_eq!(f.type_params.len(), 1);
        assert_eq!(f.type_params[0].name, "T");
        assert_eq!(f.type_params[0].constraint, Some("Loggable".to_string()));
    } else { panic!("expected function"); }
}

#[test]
fn parse_non_generic_function_unchanged() {
    let file = parse(r#"
        pub fn add(a: Number, b: Number) -> Number {
            return a + b
        test { self(1, 2) == 3 }}
    "#);
    if let Item::Function(f) = &file.items[0] {
        assert!(f.type_params.is_empty());
    } else { panic!("expected function"); }
}
