//! Type system tests (E-TYP, E-STR)
//!
//! These enforce type checking at every position:
//!   E-TYP-001: type mismatch (expected X, got Y)
//!   E-TYP-002: unknown type name
//!   E-STR-006: unknown field on struct

fn has_error(src: &str, code: &str) -> bool {
    crate::parse(src).errors.iter().any(|d| d.code == code)
}

fn is_clean(src: &str) -> bool {
    crate::parse(src).errors.is_empty()
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

// ─── Valid programs (must NOT error) ─────────────────────

#[test]
fn valid_int_arithmetic() {
    assert!(is_clean("fn add(b a: Int, b b: Int) -> Int { return a + b }"));
}

#[test]
fn valid_float_arithmetic() {
    assert!(is_clean("fn add(b a: Float, b b: Float) -> Float { return a + b }"));
}

#[test]
fn valid_bool_return() {
    assert!(is_clean("fn check(b n: Int) -> Bool { return n > 0 }"));
}

#[test]
fn valid_string_return() {
    assert!(is_clean(r#"fn greet() -> String { return "hello" }"#));
}

// ─── Binary op type mismatches ───────────────────────────

#[test]
fn binop_int_plus_string() {
    // Can't add Int + String
    assert!(has_error(r#"
        fn bad(b x: Int) -> Int {
            return x + "hello"
        }
    "#, "E-TYP-001"));
}

#[test]
fn binop_float_plus_int() {
    // Can't mix Float + Int without explicit cast
    assert!(has_error(r#"
        fn bad(b x: Float, b y: Int) -> Float {
            return x + y
        }
    "#, "E-TYP-001"));
}

#[test]
fn binop_bool_plus_int() {
    // Can't add Bool + Int
    assert!(has_error(r#"
        fn bad(b x: Bool, b y: Int) -> Int {
            return x + y
        }
    "#, "E-TYP-001"));
}

// ─── Return type mismatches ──────────────────────────────

#[test]
fn return_int_when_float_expected() {
    assert!(has_error(r#"
        fn bad() -> Float {
            return 42
        }
    "#, "E-TYP-001"));
}

#[test]
fn return_bool_when_int_expected() {
    assert!(has_error(r#"
        fn bad() -> Int {
            return true
        }
    "#, "E-TYP-001"));
}

// ─── Call argument type mismatches ───────────────────────

#[test]
fn call_wrong_arg_type() {
    // process expects Int, passing String
    assert!(has_error(r#"
        fn process(b x: Int) -> Int { return x }
        fn main() -> Int {
            let borrowed = "hello"
            return process(borrowed)
        }
    "#, "E-TYP-001"));
}

// ─── Const binding propagates type ───────────────────────

#[test]
fn const_binding_type_propagation() {
    // x is Int (from literal), return type is Int — should be clean
    assert!(is_clean(r#"
        fn double(b n: Int) -> Int {
            const x = n + n
            return x
        }
    "#));
}

#[test]
fn const_binding_wrong_return() {
    // x is String (from literal), but return type is Int
    assert!(has_error(r#"
        fn bad() -> Int {
            const x = "hello"
            return x
        }
    "#, "E-TYP-001"));
}

// ─── Comparison returns Bool ─────────────────────────────

#[test]
fn comparison_is_bool_not_int() {
    // n > 0 returns Bool, but function returns Int
    assert!(has_error(r#"
        fn bad(b n: Int) -> Int {
            return n > 0
        }
    "#, "E-TYP-001"));
}

// ─── Struct constructor return type ──────────────────────

#[test]
fn struct_constructor_returns_struct_type() {
    // Point.new returns Point, function returns Int — mismatch
    assert!(has_error(r#"
        pub struct Point { x: Int  y: Int }{
            pub fn new(o x: Int, o y: Int) -> Point {
                return Point { x: x, y: y }
            }
        }
        fn bad() -> Int {
            return Point.new(1, 2)
        }
    "#, "E-TYP-001"));
}
