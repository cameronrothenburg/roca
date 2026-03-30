use super::harness::run;

#[test]
fn generic_array_type_parses() {
    let file = roca::parse::parse(r#"
        pub struct Inbox {
            emails: Array<Email>
        }{}
    "#);
    if let roca::ast::Item::Struct(s) = &file.items[0] {
        assert!(matches!(&s.fields[0].type_ref, roca::ast::TypeRef::Generic(name, args) if name == "Array" && args.len() == 1));
    }
}

#[test]
fn generic_map_type_parses() {
    let file = roca::parse::parse(r#"
        pub struct Config {
            settings: Map<String, Number>
        }{}
    "#);
    if let roca::ast::Item::Struct(s) = &file.items[0] {
        assert!(matches!(&s.fields[0].type_ref, roca::ast::TypeRef::Generic(name, args) if name == "Map" && args.len() == 2));
    }
}

#[test]
fn generic_param_type_parses() {
    let file = roca::parse::parse(r#"
        pub fn process(items: Array<String>) -> Number {
            return items.length
            test { self(["a"]) == 1 }
        }
    "#);
    if let roca::ast::Item::Function(f) = &file.items[0] {
        assert!(matches!(&f.params[0].type_ref, roca::ast::TypeRef::Generic(..)));
    }
}

#[test]
fn generic_return_type_parses() {
    let file = roca::parse::parse(r#"
        pub fn make() -> Array<Number> {
            return [1, 2, 3]
            test { self() == [1, 2, 3] }
        }
    "#);
    if let roca::ast::Item::Function(f) = &file.items[0] {
        assert!(matches!(&f.return_type, roca::ast::TypeRef::Generic(..)));
    }
}

#[test]
fn generic_array_js_execution() {
    assert_eq!(run(
        r#"pub fn first(items: Array<String>) -> String {
            return items[0]
            test { self(["hello"]) == "hello" }
        }"#,
        r#"console.log(first(["a", "b", "c"]));"#,
    ), "a");
}

#[test]
fn nullable_generic() {
    let file = roca::parse::parse(r#"
        pub struct Box {
            items: Array<String> | null
        }{}
    "#);
    if let roca::ast::Item::Struct(s) = &file.items[0] {
        assert!(matches!(&s.fields[0].type_ref, roca::ast::TypeRef::Nullable(_)));
    }
}
