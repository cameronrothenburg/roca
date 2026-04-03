//! Control flow, data structures, string methods, arrays, crash, and test runner tests

use super::test_helpers::*;
use crate::test_runner;

// ── Control flow ──────────────────────────────────────────────────────

#[test]
fn if_else() {
    let mut m = jit(r#"
        pub fn clamp(n: Number) -> Number {
            if n > 100 { return 100 }
            if n < 0 { return 0 }
            return n
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "clamp")) };
    assert_eq!(f(50.0), 50.0);
    assert_eq!(f(150.0), 100.0);
    assert_eq!(f(-10.0), 0.0);
}

#[test]
fn nested_if_else() {
    let mut m = jit(r#"
        pub fn classify(n: Number) -> Number {
            if n > 0 {
                if n > 100 {
                    return 2
                }
                return 1
            } else {
                return 0
            }
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "classify")) };
    assert_eq!(f(50.0), 1.0);
    assert_eq!(f(200.0), 2.0);
    assert_eq!(f(-5.0), 0.0);
}

#[test]
fn while_loop() {
    let mut m = jit(r#"
        pub fn count_to(n: Number) -> Number {
            let i = 0
            while i < n { i = i + 1 }
            return i
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "count_to")) };
    assert_eq!(f(5.0), 5.0);
    assert_eq!(f(100.0), 100.0);
}

#[test]
fn break_in_while() {
    let mut m = jit(r#"
        pub fn find_five(n: Number) -> Number {
            let i = 0
            while i < n {
                if i == 5 { break }
                i = i + 1
            }
            return i
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "find_five")) };
    assert_eq!(f(10.0), 5.0);
    assert_eq!(f(3.0), 3.0);
}

#[test]
fn continue_in_loop() {
    let mut m = jit(r#"
        pub fn sum_skip_three(n: Number) -> Number {
            let total = 0
            let i = 0
            while i < n {
                i = i + 1
                if i == 3 { continue }
                total = total + i
            }
            return total
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "sum_skip_three")) };
    // 1 + 2 + 4 + 5 = 12
    assert_eq!(f(5.0), 12.0);
}

#[test]
fn match_expression() {
    let mut m = jit(r#"
        pub fn describe(n: Number) -> Number {
            const result = match n {
                1 => 10
                2 => 20
                _ => 0
            }
            return result
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "describe")) };
    assert_eq!(f(1.0), 10.0);
    assert_eq!(f(2.0), 20.0);
    assert_eq!(f(99.0), 0.0);
}

#[test]
fn multiple_match_types() {
    let mut m = jit(r#"
        pub fn label(s: String) -> String {
            return match s {
                "a" => "alpha"
                "b" => "beta"
                _ => "unknown"
            }
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> *const u8>(get_function_ptr(&m, "label").unwrap()) };
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(f(b"a\0".as_ptr()) as *const i8) }.to_str().unwrap(), "alpha");
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(f(b"b\0".as_ptr()) as *const i8) }.to_str().unwrap(), "beta");
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(f(b"x\0".as_ptr()) as *const i8) }.to_str().unwrap(), "unknown");
}

// ── String methods ────────────────────────────────────────────────────

#[test]
fn string_length() {
    let mut m = jit(r#"
        pub fn len(s: String) -> Number {
            return s.length
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> f64>(get_function_ptr(&m, "len").unwrap()) };
    assert_eq!(f(b"hello\0".as_ptr()), 5.0);
    assert_eq!(f(b"\0".as_ptr()), 0.0);
}

#[test]
fn string_includes() {
    let mut m = jit(r#"
        pub fn has_world(s: String) -> Number {
            if s.includes("world") { return 1 }
            return 0
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> f64>(get_function_ptr(&m, "has_world").unwrap()) };
    assert_eq!(f(b"hello world\0".as_ptr()), 1.0);
    assert_eq!(f(b"hello\0".as_ptr()), 0.0);
}

#[test]
fn string_trim_upper_lower() {
    let mut m = jit(r#"
        pub fn clean(s: String) -> String {
            return s.trim().toUpperCase()
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> *const u8>(get_function_ptr(&m, "clean").unwrap()) };
    let result = f(b"  hello  \0".as_ptr());
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "HELLO");
}

#[test]
fn string_slice() {
    let mut m = jit(r#"
        pub fn first_three(s: String) -> String {
            return s.slice(0, 3)
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> *const u8>(get_function_ptr(&m, "first_three").unwrap()) };
    let result = f(b"abcdef\0".as_ptr());
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "abc");
}

#[test]
fn string_index_of() {
    let mut m = jit(r#"
        pub fn find_pos(s: String) -> Number {
            return s.indexOf("world")
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> f64>(get_function_ptr(&m, "find_pos").unwrap()) };
    assert_eq!(f(b"hello world\0".as_ptr()), 6.0);
    assert_eq!(f(b"hello\0".as_ptr()), -1.0);
}

#[test]
fn chained_string_methods() {
    let mut m = jit(r#"
        pub fn process(s: String) -> String {
            return s.trim().toLowerCase()
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> *const u8>(get_function_ptr(&m, "process").unwrap()) };
    let result = f(b"  HELLO WORLD  \0".as_ptr());
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "hello world");
}

// ── Arrays and structs ────────────────────────────────────────────────

#[test]
fn array_literal_and_index() {
    let mut m = jit(r#"
        pub fn second() -> Number {
            const arr = [10, 20, 30]
            return arr[1]
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "second")) };
    assert_eq!(f(), 20.0);
}

#[test]
fn array_push_and_len() {
    let mut m = jit(r#"
        pub fn build() -> Number {
            const arr = [1, 2]
            arr.push(3)
            return arr.length
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "build")) };
    assert_eq!(f(), 3.0);
}

#[test]
fn array_map() {
    let mut m = jit(r#"
        pub fn doubled() -> Number {
            const arr = [1, 2, 3]
            const result = arr.map(fn(x) -> x * 2)
            return result[0] + result[1] + result[2]
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "doubled")) };
    assert_eq!(f(), 12.0); // 2 + 4 + 6
}

#[test]
fn array_filter() {
    let mut m = jit(r#"
        pub fn count_all() -> Number {
            const arr = [1, 2, 3]
            const result = arr.filter(fn(x) -> x > 0)
            return result.length
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "count_all")) };
    assert_eq!(f(), 3.0);
}

#[test]
fn struct_create_and_access() {
    let mut m = jit(r#"
        pub fn get_x() -> Number {
            const p = Point { x: 10, y: 20 }
            return p.x + p.y
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "get_x")) };
    assert_eq!(f(), 30.0);
}

// ── Error handling and crash blocks ───────────────────────────────────

#[test]
fn error_return_and_destructure() {
    let mut m = jit(r#"
        pub fn validate(n: Number) -> Number, err {
            if n < 0 { return err.negative }
            return n * 2
        }
        pub fn safe_double(n: Number) -> Number {
            let result, failed = validate(n)
            if failed { return 0 }
            return result
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "safe_double")) };
    assert_eq!(f(5.0), 10.0);
    assert_eq!(f(-3.0), 0.0);
}

#[test]
fn crash_fallback() {
    let mut m = jit(r#"
        pub fn risky(n: Number) -> Number, err {
            if n < 0 { return err.negative }
            return n * 2
        }
        pub fn safe(n: Number) -> Number {
            return risky(n)
        crash {
            risky -> fallback(0)
        }}
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "safe")) };
    assert_eq!(f(5.0), 10.0);
    assert_eq!(f(-3.0), 0.0);
}

#[test]
fn crash_halt_propagates() {
    let mut m = jit(r#"
        pub fn inner(n: Number) -> Number, err {
            if n == 0 { return err.zero }
            return 100 / n
        }
        pub fn outer(n: Number) -> Number, err {
            return inner(n)
        crash {
            inner -> halt
        }}
    "#);
    // Call outer with error — should propagate
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> (f64, u8)>(get_function_ptr(&m, "outer").unwrap()) };
    let (val, err) = f(5.0);
    assert_eq!(val, 20.0);
    assert_eq!(err, 0);
    let (_val, err) = f(0.0);
    assert_ne!(err, 0); // Error propagated
}

// ── Test runner and auto-stubs ────────────────────────────────────────

#[test]
fn native_test_runner_equality() {
    let source = roca_parse::parse(r#"
        pub fn add(a: Number, b: Number) -> Number {
            return a + b
        test {
            self(1, 2) == 3
            self(0, 0) == 0
            self(-1, 1) == 0
        }}
    "#);
    let result = test_runner::run_tests(&source);
    assert!(result.passed >= 3, "expected >= 3 passed (3 cases + property tests), got {}: {}", result.passed, result.output);
    assert_eq!(result.failed, 0, "output: {}", result.output);
}

#[test]
fn native_test_runner_err() {
    let source = roca_parse::parse(r#"
        pub fn validate(n: Number) -> Number, err {
            if n < 0 { return err.negative }
            return n
        test {
            self(5) == 5
            self(-1) is err.negative
            self(0) is Ok
        }}
    "#);
    let result = test_runner::run_tests(&source);
    assert!(result.passed >= 3, "expected >= 3 passed, got {}: {}", result.passed, result.output);
    assert_eq!(result.failed, 0, "output: {}", result.output);
}

#[test]
fn native_test_runner_failing() {
    let source = roca_parse::parse(r#"
        pub fn double(n: Number) -> Number {
            return n * 3
        test {
            self(2) == 4
        }}
    "#);
    let result = test_runner::run_tests(&source);
    // Explicit test fails, but property tests still run and pass (function doesn't crash)
    assert!(result.failed >= 1, "expected >= 1 failure");
}

#[test]
fn auto_stub_extern_fn() {
    // Auto-stub returns default for Number (0.0)
    let mut m = jit(r#"
        extern fn fetch_price() -> Number
        pub fn get_price() -> Number {
            return fetch_price()
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "get_price")) };
    assert_eq!(f(), 0.0); // auto-stub returns default Number
}

#[test]
fn auto_stub_extern_fn_with_err() {
    // Auto-stub returns default String ("") with no error — crash fallback not triggered
    let source = roca_parse::parse(r#"
        extern fn load(id: Number) -> String, err {
            err not_found = "not found"
        }
        pub fn safe_load(id: Number) -> String {
            return load(id)
        crash {
            load -> fallback("default")
        }
        test {
            self(1) == ""
        }}
    "#);
    let result = test_runner::run_tests(&source);
    assert!(result.passed >= 1, "expected >= 1 passed, got {}: {}", result.passed, result.output);
    assert_eq!(result.failed, 0, "output: {}", result.output);
}
