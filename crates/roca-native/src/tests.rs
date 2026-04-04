//! TDD — 10 tests defining roca-native's contract.

use crate::{compile, call, run_tests, Value};

fn compile_src(src: &str) -> crate::Module {
    let result = roca_parse::parse(src);
    assert!(result.errors.is_empty(), "parse errors: {:?}", result.errors);
    compile(&result.ast).expect("compile failed")
}

#[test]
fn return_int() {
    let m = compile_src("fn answer() -> Int { return 42 }");
    assert_eq!(call(&m, "answer", &[]), Value::Int(42));
}

#[test]
fn return_float() {
    let m = compile_src("fn pi() -> Float { return 3.14 }");
    if let Value::Float(f) = call(&m, "pi", &[]) {
        assert!((f - 3.14).abs() < 1e-10);
    } else {
        panic!("expected Float");
    }
}

#[test]
fn arithmetic() {
    let m = compile_src("fn math(b a: Int, b b: Int) -> Int { return a + b * 2 }");
    assert_eq!(call(&m, "math", &[3, 4]), Value::Int(11));
}

#[test]
fn boolean_logic() {
    let m = compile_src("fn is_positive(b n: Int) -> Bool { return n > 0 }");
    assert_eq!(call(&m, "is_positive", &[5]), Value::Bool(true));
    assert_eq!(call(&m, "is_positive", &[-3]), Value::Bool(false));
}

#[test]
fn if_else() {
    let m = compile_src(r#"
        fn abs(b n: Int) -> Int {
            if n < 0 { return 0 - n }
            return n
        }
    "#);
    assert_eq!(call(&m, "abs", &[5]), Value::Int(5));
    assert_eq!(call(&m, "abs", &[-7]), Value::Int(7));
}

#[test]
fn const_binding() {
    let m = compile_src(r#"
        fn double(b x: Int) -> Int {
            const result = x + x
            return result
        }
    "#);
    assert_eq!(call(&m, "double", &[21]), Value::Int(42));
}

#[test]
fn struct_create_and_field_access() {
    let m = compile_src(r#"
        pub struct Point { x: Int  y: Int }{
            pub fn new(o x: Int, o y: Int) -> Point {
                return Point { x: x, y: y }
            }
        }
        fn get_sum() -> Int {
            const p = Point.new(10, 20)
            const x = p.x
            const y = p.y
            return x + y
        }
    "#);
    assert_eq!(call(&m, "get_sum", &[]), Value::Int(30));
}

#[test]
fn string_return() {
    let m = compile_src(r#"fn greeting() -> String { return "hello" }"#);
    if let Value::String(ptr) = call(&m, "greeting", &[]) {
        assert_ne!(ptr, 0, "string should not be null");
        let s = roca_mem::read_cstr(ptr);
        assert_eq!(s, "hello");
    } else {
        panic!("expected String");
    }
}

#[test]
fn loop_sum() {
    let m = compile_src(r#"
        fn sum_to(b n: Int) -> Int {
            var total = 0
            var i = 0
            loop {
                if i >= n { break }
                total = total + i
                i = i + 1
            }
            return total
        }
    "#);
    assert_eq!(call(&m, "sum_to", &[5]), Value::Int(10));
}

#[test]
fn proof_test() {
    let result = roca_parse::parse(r#"
        pub fn add(b a: Int, b b: Int) -> Int {
            return a + b
        test {
            self(1, 2) == 3
            self(0, 0) == 0
        }}
    "#);
    assert!(result.errors.is_empty(), "parse errors: {:?}", result.errors);
    let test_result = run_tests(&result.ast);
    assert_eq!(test_result.passed, 2, "expected 2 passed: {}", test_result.output);
    assert_eq!(test_result.failed, 0, "expected 0 failed: {}", test_result.output);
}
