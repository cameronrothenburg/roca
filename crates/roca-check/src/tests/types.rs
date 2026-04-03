//! Type system tests (E-TYP, E-STR)
//!
//! These enforce basic type checking:
//!   E-TYP-001: type mismatch (expected X, got Y)
//!   E-TYP-002: unknown type name
//!   E-STR-006: unknown field on struct

use crate::check;

fn has_error(src: &str, code: &str) -> bool {
    let ast = roca_parse::parse(src);
    check(&ast).iter().any(|d| d.code == code)
}

#[test]
fn type_mismatch_return() {
    // Function declares -> Int but returns a String
    assert!(has_error(r#"
        fn wrong() -> Int {
            return "hello"
        }
    "#, "E-TYP-001"));
}

#[test]
fn unknown_type_name() {
    // Param uses a type that doesn't exist
    assert!(has_error(r#"
        fn process(b x: Foo) -> Int {
            return 0
        }
    "#, "E-TYP-002"));
}

#[test]
fn unknown_struct_field() {
    // Access a field that doesn't exist on the struct
    assert!(has_error(r#"
        pub struct User { name: String }{}
        fn main() -> String {
            const u = User { name: "alice" }
            return u.nonexistent
        }
    "#, "E-STR-006"));
}
