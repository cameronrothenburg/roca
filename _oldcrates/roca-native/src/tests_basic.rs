//! Core language tests — primitives, operators, bindings, strings, function calls

use super::test_helpers::*;

#[test]
fn init() { drop(create_jit_module()); }

#[test]
fn return_constant() {
    let mut m = jit("pub fn answer() -> Number { return 42 }");
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "answer")) };
    assert_eq!(f(), 42.0);
}

#[test]
fn add() {
    let mut m = jit("pub fn add(a: Number, b: Number) -> Number { return a + b }");
    let f = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "add")) };
    assert_eq!(f(37.0, 5.0), 42.0);
    assert_eq!(f(-10.0, 10.0), 0.0);
}

#[test]
fn multiply() {
    let mut m = jit("pub fn square(n: Number) -> Number { return n * n }");
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "square")) };
    assert_eq!(f(5.0), 25.0);
    assert_eq!(f(-3.0), 9.0);
}

#[test]
fn modulo_and_subtraction() {
    let mut m = jit(r#"
        pub fn sub(a: Number, b: Number) -> Number { return a - b }
        pub fn div(a: Number, b: Number) -> Number { return a / b }
    "#);
    let sub = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "sub")) };
    let div = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "div")) };
    assert_eq!(sub(10.0, 3.0), 7.0);
    assert_eq!(div(10.0, 2.0), 5.0);
}

#[test]
fn const_binding() {
    let mut m = jit(r#"
        pub fn double_add(a: Number, b: Number) -> Number {
            const sum = a + b
            return sum + sum
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "double_add")) };
    assert_eq!(f(3.0, 4.0), 14.0);
}

#[test]
fn not_operator() {
    let mut m = jit(r#"
        pub fn negate(n: Number) -> Number {
            if !(n > 0) { return 1 }
            return 0
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "negate")) };
    assert_eq!(f(-5.0), 1.0);
    assert_eq!(f(5.0), 0.0);
}

#[test]
fn and_or() {
    let mut m = jit(r#"
        pub fn both(a: Number, b: Number) -> Number {
            if a > 0 && b > 0 { return 1 }
            return 0
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "both")) };
    assert_eq!(f(1.0, 1.0), 1.0);
    assert_eq!(f(1.0, -1.0), 0.0);
}

#[test]
fn function_calls() {
    let mut m = jit(r#"
        pub fn add(a: Number, b: Number) -> Number { return a + b }
        pub fn double(n: Number) -> Number { return add(n, n) }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "double")) };
    assert_eq!(f(5.0), 10.0);
    assert_eq!(f(21.0), 42.0);
}

#[test]
fn string_literal() {
    let mut m = jit(r#"pub fn greeting() -> String { return "hello" }"#);
    let f = unsafe { std::mem::transmute::<_, fn() -> i64>(get_function_ptr(&m, "greeting").unwrap()) };
    let result = f() as *const u8;
    assert!(!result.is_null());
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "hello");
}

#[test]
fn string_equality() {
    let mut m = jit(r#"
        pub fn is_hello(s: String) -> Bool {
            if s == "hello" { return true }
            return false
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> i8>(get_function_ptr(&m, "is_hello").unwrap()) };
    assert_eq!(f(b"hello\0".as_ptr()), 1);
    assert_eq!(f(b"world\0".as_ptr()), 0);
}

#[test]
fn string_concat() {
    let mut m = jit(r#"
        pub fn greet(name: String) -> String {
            return "hello " + name
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> i64>(get_function_ptr(&m, "greet").unwrap()) };
    let result = f(b"world\0".as_ptr()) as *const u8;
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "hello world");
}

#[test]
fn string_interpolation() {
    let mut m = jit(r#"
        pub fn greet(name: String) -> String {
            return "hello {name}!"
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> i64>(get_function_ptr(&m, "greet").unwrap()) };
    let result = f(b"world\0".as_ptr()) as *const u8;
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "hello world!");
}

#[test]
fn number_to_string() {
    let mut m = jit(r#"
        pub fn show(n: Number) -> String {
            return "{n} items"
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> i64>(get_function_ptr(&m, "show").unwrap()) };
    let result = f(42.0) as *const u8;
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "42 items");
}

#[test]
fn method_to_string() {
    let mut m = jit(r#"
        pub fn num_to_str(n: Number) -> String {
            return n.toString()
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> i64>(get_function_ptr(&m, "num_to_str").unwrap()) };
    let result = f(42.0) as *const u8;
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "42");
}
