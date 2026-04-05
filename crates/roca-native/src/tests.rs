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
    assert_eq!(call(&m, "math", &[Value::Int(3), Value::Int(4)]), Value::Int(11));
}

#[test]
fn boolean_logic() {
    let m = compile_src("fn is_positive(b n: Int) -> Bool { return n > 0 }");
    assert_eq!(call(&m, "is_positive", &[Value::Int(5)]), Value::Bool(true));
    assert_eq!(call(&m, "is_positive", &[Value::Int(-3)]), Value::Bool(false));
}

#[test]
fn if_else() {
    let m = compile_src(r#"
        fn abs(b n: Int) -> Int {
            if n < 0 { return 0 - n }
            return n
        }
    "#);
    assert_eq!(call(&m, "abs", &[Value::Int(5)]), Value::Int(5));
    assert_eq!(call(&m, "abs", &[Value::Int(-7)]), Value::Int(7));
}

#[test]
fn const_binding() {
    let m = compile_src(r#"
        fn double(b x: Int) -> Int {
            const result = x + x
            return result
        }
    "#);
    assert_eq!(call(&m, "double", &[Value::Int(21)]), Value::Int(42));
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
    assert_eq!(call(&m, "sum_to", &[Value::Int(5)]), Value::Int(10));
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
    assert_eq!(call(&m, "quad", &[Value::Int(3)]), Value::Int(12));
}

#[test]
fn multiple_return_paths() {
    let m = compile_src(r#"
        fn max(b a: Int, b b: Int) -> Int {
            if a > b { return a }
            return b
        }
    "#);
    assert_eq!(call(&m, "max", &[Value::Int(5), Value::Int(3)]), Value::Int(5));
    assert_eq!(call(&m, "max", &[Value::Int(2), Value::Int(7)]), Value::Int(7));
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
    assert_eq!(call(&m, "helper", &[Value::Int(42)]), Value::Int(42));
}

#[test]
fn unary_negation() {
    let m = compile_src(r#"
        fn neg(b x: Int) -> Int { return 0 - x }
    "#);
    assert_eq!(call(&m, "neg", &[Value::Int(5)]), Value::Int(-5));
}

#[test]
fn comparison_returns_bool() {
    let m = compile_src(r#"
        fn eq(b a: Int, b b: Int) -> Bool { return a == b }
    "#);
    assert_eq!(call(&m, "eq", &[Value::Int(5), Value::Int(5)]), Value::Bool(true));
    assert_eq!(call(&m, "eq", &[Value::Int(5), Value::Int(3)]), Value::Bool(false));
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

// ─── Shim: arbitrary arg count ──────────────────────────

#[test]
fn call_with_five_args() {
    let m = compile_src(r#"
        fn sum5(b a: Int, b b: Int, b c: Int, b d: Int, b e: Int) -> Int {
            return a + b + c + d + e
        }
    "#);
    assert_eq!(
        call(&m, "sum5", &[Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4), Value::Int(5)]),
        Value::Int(15)
    );
}

#[test]
fn call_with_eight_args() {
    let m = compile_src(r#"
        fn sum8(b a: Int, b b: Int, b c: Int, b d: Int, b e: Int, b f: Int, b g: Int, b h: Int) -> Int {
            return a + b + c + d + e + f + g + h
        }
    "#);
    assert_eq!(
        call(&m, "sum8", &[
            Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4),
            Value::Int(5), Value::Int(6), Value::Int(7), Value::Int(8),
        ]),
        Value::Int(36)
    );
}

#[test]
fn call_mixed_int_and_bool_params() {
    let m = compile_src(r#"
        fn choose(b flag: Bool, b a: Int, b b: Int) -> Int {
            if flag { return a }
            return b
        }
    "#);
    assert_eq!(
        call(&m, "choose", &[Value::Bool(true), Value::Int(10), Value::Int(20)]),
        Value::Int(10)
    );
    assert_eq!(
        call(&m, "choose", &[Value::Bool(false), Value::Int(10), Value::Int(20)]),
        Value::Int(20)
    );
}

#[test]
fn call_six_mixed_params() {
    let m = compile_src(r#"
        fn mix(b a: Int, b b: Bool, b c: Int, b d: Bool, b e: Int, b f: Int) -> Int {
            var result = a + c + e + f
            if b { result = result + 100 }
            if d { result = result + 200 }
            return result
        }
    "#);
    assert_eq!(
        call(&m, "mix", &[
            Value::Int(1), Value::Bool(true), Value::Int(2), Value::Bool(false),
            Value::Int(3), Value::Int(4),
        ]),
        Value::Int(110) // 1+2+3+4=10, b=true adds 100, d=false adds 0
    );
}

#[test]
fn call_float_params() {
    let m = compile_src(r#"
        fn add_floats(b a: Float, b b: Float) -> Float {
            return a + b
        }
    "#);
    if let Value::Float(f) = call(&m, "add_floats", &[Value::Float(1.5), Value::Float(2.5)]) {
        assert!((f - 4.0).abs() < 1e-10);
    } else {
        panic!("expected Float");
    }
}

#[test]
fn call_int_returning_float() {
    let m = compile_src(r#"
        fn to_float(b n: Int) -> Float {
            return 3.14
        }
    "#);
    if let Value::Float(f) = call(&m, "to_float", &[Value::Int(0)]) {
        assert!((f - 3.14).abs() < 1e-10);
    } else {
        panic!("expected Float");
    }
}

// ─── Red tests: known bugs from examples ────────────────

#[test]
fn negative_literal_in_test_args() {
    // Bug: self(-1, 1) parses -1 as two tokens, not a negative literal
    let result = roca_parse::parse(r#"
        pub fn add(b a: Int, b b: Int) -> Int {
            return a + b
        test {
            self(-1, 1) == 0
        }}
    "#);
    assert!(result.errors.is_empty(), "parse errors: {:?}", result.errors);
    let test_result = run_tests(&result.ast);
    assert_eq!(test_result.passed, 1, "negative args should work: {}", test_result.output);
    assert_eq!(test_result.failed, 0, "{}", test_result.output);
}

#[test]
fn closure_captures_param() {
    // Bug: closures don't use params from enclosing function
    let m = compile_src(r#"
        fn apply(b x: Int) -> Int {
            const double = fn(n) -> n * 2
            return double(x)
        }
    "#);
    assert_eq!(call(&m, "apply", &[Value::Int(5)]), Value::Int(10));
}

#[test]
fn string_equality_in_proof_test() {
    // Bug: test runner compares strings as Int(0) instead of String values
    let result = roca_parse::parse(r#"
        pub fn greeting() -> String {
            return "hello"
        test {
            self() == "hello"
        }}
    "#);
    assert!(result.errors.is_empty(), "parse errors: {:?}", result.errors);
    let test_result = run_tests(&result.ast);
    assert_eq!(test_result.passed, 1, "string equality should work: {}", test_result.output);
    assert_eq!(test_result.failed, 0, "{}", test_result.output);
}

#[test]
fn match_returning_string() {
    // Bug: match with string arms doesn't emit correct types
    let m = compile_src(r#"
        fn describe(b n: Int) -> String {
            const result = match n {
                1 => "one"
                2 => "two"
                _ => "other"
            }
            return result
        }
    "#);
    let val = call(&m, "describe", &[Value::Int(1)]);
    if let Value::String(ptr) = val {
        assert_ne!(ptr, 0, "should return non-null string");
        assert_eq!(roca_mem::read_cstr(ptr), "one");
    } else {
        panic!("expected String, got {:?}", val);
    }
}

#[test]
fn struct_instance_method_with_self() {
    // Bug: instance methods using self.field crash Cranelift verifier
    let m = compile_src(r#"
        pub struct Counter { value: Int }{
            pub fn new(o v: Int) -> Counter {
                return Counter { value: v }
            }
            pub fn get() -> Int {
                return self.value
            }
        }
        fn test_it() -> Int {
            const c = Counter.new(42)
            return c.get()
        }
    "#);
    assert_eq!(call(&m, "test_it", &[]), Value::Int(42));
}

// ─── Red tests: compiler gaps ───────────────────────────

#[test]
fn match_int_arms() {
    let m = compile_src(r#"
        fn pick(b n: Int) -> Int {
            return match n {
                1 => 10
                2 => 20
                _ => 99
            }
        }
    "#);
    assert_eq!(call(&m, "pick", &[Value::Int(1)]), Value::Int(10));
    assert_eq!(call(&m, "pick", &[Value::Int(2)]), Value::Int(20));
    assert_eq!(call(&m, "pick", &[Value::Int(7)]), Value::Int(99));
}

#[test]
fn match_with_wildcard_only() {
    let m = compile_src(r#"
        fn always(b n: Int) -> Int {
            return match n {
                _ => 42
            }
        }
    "#);
    assert_eq!(call(&m, "always", &[Value::Int(0)]), Value::Int(42));
}

#[test]
fn struct_method_reads_field() {
    let m = compile_src(r#"
        pub struct Box { value: Int }{
            pub fn new(o v: Int) -> Box {
                return Box { value: v }
            }
            pub fn get() -> Int {
                return self.value
            }
        }
        fn test_get() -> Int {
            const b = Box.new(99)
            return b.get()
        }
    "#);
    assert_eq!(call(&m, "test_get", &[]), Value::Int(99));
}

#[test]
fn struct_method_two_fields() {
    let m = compile_src(r#"
        pub struct Pair { x: Int  y: Int }{
            pub fn new(o x: Int, o y: Int) -> Pair {
                return Pair { x: x, y: y }
            }
            pub fn sum() -> Int {
                return self.x + self.y
            }
        }
        fn test_sum() -> Int {
            const p = Pair.new(3, 7)
            return p.sum()
        }
    "#);
    assert_eq!(call(&m, "test_sum", &[]), Value::Int(10));
}

#[test]
fn closure_identity() {
    let m = compile_src(r#"
        fn test_id(b x: Int) -> Int {
            const id = fn(n) -> n
            return id(x)
        }
    "#);
    assert_eq!(call(&m, "test_id", &[Value::Int(7)]), Value::Int(7));
}

#[test]
fn struct_field_mutation() {
    let m = compile_src(r#"
        pub struct Counter { value: Int }{
            pub fn new(o v: Int) -> Counter {
                return Counter { value: v }
            }
            pub fn inc() -> Int {
                self.value = self.value + 1
                return self.value
            }
        }
        fn test_inc() -> Int {
            const c = Counter.new(10)
            return c.inc()
        }
    "#);
    assert_eq!(call(&m, "test_inc", &[]), Value::Int(11));
}

#[test]
fn if_expression_value() {
    let m = compile_src(r#"
        fn pick(b cond: Bool) -> Int {
            const x = if cond { 1 } else { 2 }
            return x
        }
    "#);
    assert_eq!(call(&m, "pick", &[Value::Bool(true)]), Value::Int(1));
    assert_eq!(call(&m, "pick", &[Value::Bool(false)]), Value::Int(2));
}
