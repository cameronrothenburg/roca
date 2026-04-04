//! TDD Red — 10 tests verifying correct JS emission from Roca AST.
//!
//! Each test parses Roca source, emits JS, and checks the output.
//! The emitter builds an OXC JS AST internally — these tests verify
//! the final rendered JS is correct.

fn emit_src(src: &str) -> String {
    let result = roca_parse::parse(src);
    assert!(result.errors.is_empty(), "parse errors: {:?}", result.errors);
    crate::emit(&result.ast)
}

// ─── 1. Basic function ──────────────────────────────────

#[test]
fn emit_function() {
    let js = emit_src("fn add(b a: Int, b b: Int) -> Int { return a + b }");
    assert!(js.contains("function add(a, b)"), "expected function declaration, got:\n{js}");
    assert!(js.contains("return a + b"), "expected return statement, got:\n{js}");
}

// ─── 2. Pub = export ────────────────────────────────────

#[test]
fn emit_pub_function_exported() {
    let js = emit_src(r#"pub fn greet() -> String { return "hello" }"#);
    assert!(js.contains("export"), "pub fn should produce export, got:\n{js}");
    assert!(js.contains("function greet()"), "expected greet function, got:\n{js}");
}

// ─── 3. String literal ─────────────────────────────────

#[test]
fn emit_string_literal() {
    let js = emit_src(r#"fn msg() -> String { return "hello world" }"#);
    assert!(js.contains("\"hello world\""), "expected string literal, got:\n{js}");
}

// ─── 4. Const binding ──────────────────────────────────

#[test]
fn emit_const_binding() {
    let js = emit_src(r#"
        fn double(b x: Int) -> Int {
            const result = x + x
            return result
        }
    "#);
    assert!(js.contains("const result"), "expected const declaration, got:\n{js}");
    assert!(js.contains("x + x"), "expected addition, got:\n{js}");
}

// ─── 5. If/else ─────────────────────────────────────────

#[test]
fn emit_if_else() {
    let js = emit_src(r#"
        fn abs(b n: Int) -> Int {
            if n < 0 { return 0 - n }
            return n
        }
    "#);
    assert!(js.contains("if"), "expected if statement, got:\n{js}");
    assert!(js.contains("n < 0") || js.contains("n<0"), "expected condition, got:\n{js}");
}

// ─── 6. Struct as class ─────────────────────────────────

#[test]
fn emit_struct_as_class() {
    let js = emit_src(r#"
        pub struct Point { x: Int  y: Int }{
            pub fn new(o x: Int, o y: Int) -> Point {
                return Point { x: x, y: y }
            }
        }
    "#);
    assert!(js.contains("class Point"), "expected class declaration, got:\n{js}");
}

// ─── 7. For loop ────────────────────────────────────────

#[test]
fn emit_for_loop() {
    let js = emit_src(r#"
        fn sum(b items: Array) -> Int {
            var total = 0
            for item in items {
                total = total + item
            }
            return total
        }
    "#);
    assert!(js.contains("for") && js.contains("of"), "expected for-of loop, got:\n{js}");
}

// ─── 8. Match as conditional ────────────────────────────

#[test]
fn emit_match_as_conditional() {
    let js = emit_src(r#"
        fn describe(b n: Int) -> String {
            const result = match n {
                1 => "one"
                2 => "two"
                _ => "other"
            }
            return result
        }
    "#);
    // Match should emit as ternary chain or if/else
    assert!(js.contains("===") || js.contains("==") || js.contains("?"),
        "expected comparison from match, got:\n{js}");
}

// ─── 9. Closure as arrow function ───────────────────────

#[test]
fn emit_closure() {
    let js = emit_src(r#"
        fn apply(b x: Int) -> Int {
            const double = fn(n) -> n * 2
            return double(x)
        }
    "#);
    assert!(js.contains("=>"), "expected arrow function, got:\n{js}");
}

// ─── 10. Import with extension change ───────────────────

#[test]
fn emit_import() {
    let js = emit_src(r#"
        import { User } from "./types.roca"
    "#);
    assert!(js.contains("import"), "expected import statement, got:\n{js}");
    assert!(js.contains("./types.js"), "expected .roca → .js extension change, got:\n{js}");
}
