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

// ─── Additional native tests ─────────────────────────────

#[test]
fn nested_function_calls() {
    let m = compile_src(r#"
        fn double(b x: Int) -> Int { return x + x }
        fn quad(b x: Int) -> Int { return double(double(x)) }
    "#);
    assert_eq!(call(&m, "quad", &[3]), Value::Int(12));
}

#[test]
fn multiple_return_paths() {
    let m = compile_src(r#"
        fn max(b a: Int, b b: Int) -> Int {
            if a > b { return a }
            return b
        }
    "#);
    assert_eq!(call(&m, "max", &[5, 3]), Value::Int(5));
    assert_eq!(call(&m, "max", &[2, 7]), Value::Int(7));
}

#[test]
fn var_reassignment() {
    let m = compile_src(r#"
        fn count() -> Int {
            var x = 0
            x = x + 1
            x = x + 1
            x = x + 1
            return x
        }
    "#);
    assert_eq!(call(&m, "count", &[]), Value::Int(3));
}

#[test]
fn proof_test_with_failing_case() {
    let result = roca_parse::parse(r#"
        pub fn add(b a: Int, b b: Int) -> Int {
            return a + b
        test {
            self(1, 2) == 3
            self(1, 2) == 999
        }}
    "#);
    assert!(result.errors.is_empty());
    let test_result = run_tests(&result.ast);
    assert_eq!(test_result.passed, 1);
    assert_eq!(test_result.failed, 1);
}

#[test]
fn function_with_no_test_block_compiles() {
    let m = compile_src(r#"
        fn helper(b x: Int) -> Int { return x }
    "#);
    assert_eq!(call(&m, "helper", &[42]), Value::Int(42));
}

#[test]
fn unary_negation() {
    let m = compile_src(r#"
        fn neg(b x: Int) -> Int { return 0 - x }
    "#);
    assert_eq!(call(&m, "neg", &[5]), Value::Int(-5));
}

#[test]
fn comparison_returns_bool() {
    let m = compile_src(r#"
        fn eq(b a: Int, b b: Int) -> Bool { return a == b }
    "#);
    assert_eq!(call(&m, "eq", &[5, 5]), Value::Bool(true));
    assert_eq!(call(&m, "eq", &[5, 3]), Value::Bool(false));
}

#[test]
fn empty_loop_with_immediate_break() {
    let m = compile_src(r#"
        fn instant() -> Int {
            var x = 42
            loop { break }
            return x
        }
    "#);
    assert_eq!(call(&m, "instant", &[]), Value::Int(42));
}
