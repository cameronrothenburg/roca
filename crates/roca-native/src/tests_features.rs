//! Advanced feature tests: enums, structs, closures, forward refs

use super::test_helpers::*;

#[test]
fn closure_as_value() {
    let mut m = jit(r#"
        pub fn apply() -> Number {
            const double = fn(x) -> x * 2
            return double(5)
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "apply")) };
    assert_eq!(f(), 10.0);
}

#[test]
fn closure_arithmetic() {
    let mut m = jit(r#"
        pub fn compute() -> Number {
            const add_ten = fn(x) -> x + 10
            const sub_one = fn(x) -> x - 1
            return add_ten(sub_one(5))
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "compute")) };
    assert_eq!(f(), 14.0); // (5-1)+10
}

#[test]
fn closure_passed_to_function() {
    let mut m = jit(r#"
        pub fn apply_fn(n: Number, transform: fn(Number) -> Number) -> Number {
            return transform(n)
        }
        pub fn use_it() -> Number {
            const triple = fn(x) -> x * 3
            return apply_fn(4, triple)
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "use_it")) };
    assert_eq!(f(), 12.0);
}

#[test]
// ─── Integration Tests (real coding patterns) ──────

#[test]
fn enum_variant_unit() {
    let mut m = jit(r#"
        enum Token { Number(Number) Plus Minus }
        pub fn test_unit() -> Number {
            const t = Token.Plus
            return match t {
                Token.Plus => 1
                _ => 0
            }
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "test_unit")) };
    assert_eq!(f(), 1.0);
}

#[test]
fn enum_variant_with_data() {
    let mut m = jit(r#"
        enum Token { Number(Number) Plus Minus }
        pub fn test_data() -> Number {
            const t = Token.Number(42)
            return match t {
                Token.Number(n) => n
                _ => 0
            }
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "test_data")) };
    assert_eq!(f(), 42.0);
}

#[test]
fn enum_variant_multiple_arms() {
    let mut m = jit(r#"
        enum Shape { Circle(Number) Rect(Number, Number) Empty }
        pub fn describe(code: Number) -> Number {
            const shape = match code {
                1 => Shape.Circle(5)
                2 => Shape.Rect(3, 4)
                _ => Shape.Empty
            }
            return match shape {
                Shape.Circle(r) => r * r
                Shape.Rect(w, h) => w * h
                Shape.Empty => 0
                _ => 0
            }
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "describe")) };
    assert_eq!(f(1.0), 25.0);
    assert_eq!(f(2.0), 12.0);
    assert_eq!(f(3.0), 0.0);
}

#[test]
fn enum_variant_in_function_chain() {
    let mut m = jit(r#"
        enum Token { Number(Number) Plus }
        pub fn make_token(n: Number) -> Number {
            const t = Token.Number(n)
            return extract(t)
        }
        pub fn extract(t: Token) -> Number {
            return match t {
                Token.Number(v) => v
                _ => 0
            }
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "make_token")) };
    assert_eq!(f(99.0), 99.0);
}

#[test]
fn struct_method_self_read() {
    let mut m = jit(r#"
        pub struct Counter {
            count: Number
        }{
            fn current() -> Number {
                return self.count
            }
        }
        pub fn test_method() -> Number {
            const c = Counter { count: 10 }
            return Counter.current(c)
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "test_method")) };
    assert_eq!(f(), 10.0);
}

#[test]
fn struct_method_self_write() {
    let mut m = jit(r#"
        pub struct Counter {
            count: Number
        }{
            fn increment() -> Number {
                self.count = self.count + 1
                return self.count
            }
        }
        pub fn test_write() -> Number {
            let c = Counter { count: 5 }
            return Counter.increment(c)
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "test_write")) };
    assert_eq!(f(), 6.0);
}

#[test]
fn forward_reference_calls() {
    // Caller defined BEFORE callee — tests forward references
    let mut m = jit(r#"
        pub fn caller() -> Number {
            return callee(5)
        }
        pub fn callee(n: Number) -> Number {
            return n * 3
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "caller")) };
    assert_eq!(f(), 15.0);
}

#[test]
fn forward_reference_chain() {
    // A → B → C where A is defined first, C is defined last
    let mut m = jit(r#"
        pub fn step_a() -> Number {
            return step_b() + 1
        }
        pub fn step_b() -> Number {
            return step_c() + 10
        }
        pub fn step_c() -> Number {
            return 100
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "step_a")) };
    assert_eq!(f(), 111.0); // 100 + 10 + 1
}

#[test]
fn mutual_recursion() {
    // Two functions that call each other
    let mut m = jit(r#"
        pub fn is_even(n: Number) -> Number {
            if n == 0 { return 1 }
            return is_odd(n - 1)
        }
        pub fn is_odd(n: Number) -> Number {
            if n == 0 { return 0 }
            return is_even(n - 1)
        }
    "#);
    let even = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "is_even")) };
    let odd = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "is_odd")) };
    assert_eq!(even(4.0), 1.0);
    assert_eq!(even(3.0), 0.0);
    assert_eq!(odd(3.0), 1.0);
    assert_eq!(odd(4.0), 0.0);
}

#[test]
fn integration_validate_and_transform() {
    // Real pattern: validate input, transform, return or error
    let mut m = jit(r#"
        pub fn validate(n: Number) -> Number, err {
            if n < 0 { return err.negative }
            if n > 1000 { return err.too_large }
            return n
        }
        pub fn process(n: Number) -> Number {
            let result, failed = validate(n)
            if failed { return 0 }
            const doubled = result * 2
            return doubled
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "process")) };
    assert_eq!(f(5.0), 10.0);
    assert_eq!(f(-1.0), 0.0);
    assert_eq!(f(2000.0), 0.0);
}

#[test]
fn integration_loop_with_early_return() {
    // Real pattern: search a collection, return early on match
    let mut m = jit(r#"
        pub fn find_threshold(limit: Number) -> Number {
            let total = 0
            let i = 0
            while i < 20 {
                total = total + i
                if total > limit { return i }
                i = i + 1
            }
            return i
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "find_threshold")) };
    assert_eq!(f(10.0), 5.0); // 0+1+2+3+4+5=15 > 10 at i=5
    assert_eq!(f(100.0), 14.0); // 0+1+...+14=105 > 100 at i=14
}

#[test]
fn integration_string_processing_pipeline() {
    // Real pattern: chain string operations, build result
    let mut m = jit(r#"
        pub fn process_name(raw: String) -> String {
            const trimmed = raw.trim()
            const upper = trimmed.toUpperCase()
            return upper
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(i64) -> i64>(get_function_ptr(&m, "process_name").unwrap()) };
    let result = f(b"  hello  \0".as_ptr() as i64) as *const u8;
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "HELLO");
}

#[test]
fn integration_closure_with_functions() {
    // Real pattern: pass closures to utility functions
    let mut m = jit(r#"
        pub fn apply_twice(n: Number, f: fn(Number) -> Number) -> Number {
            return f(f(n))
        }
        pub fn run() -> Number {
            const inc = fn(x) -> x + 1
            const dbl = fn(x) -> x * 2
            const a = apply_twice(3, inc)
            const b = apply_twice(3, dbl)
            return a + b
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "run")) };
    assert_eq!(f(), 17.0); // apply_twice(3,inc)=5, apply_twice(3,dbl)=12, 5+12=17
}

#[test]
fn integration_multi_function_with_strings() {
    // Real pattern: multiple functions sharing string data
    let mut m = jit(r#"
        pub fn greet(name: String) -> String {
            return "hello " + name
        }
        pub fn shout(msg: String) -> String {
            return msg.toUpperCase()
        }
        pub fn pipeline(name: String) -> String {
            const greeting = greet(name)
            return shout(greeting)
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(i64) -> i64>(get_function_ptr(&m, "pipeline").unwrap()) };
    let result = f(b"world\0".as_ptr() as i64) as *const u8;
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "HELLO WORLD");
}

#[test]
fn integration_match_with_computation() {
    // Real pattern: match on value, compute differently per case
    let mut m = jit(r#"
        pub fn score(grade: Number) -> Number {
            const points = match grade {
                1 => 100
                2 => 85
                3 => 70
                _ => 50
            }
            return points * 2
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "score")) };
    assert_eq!(f(1.0), 200.0);
    assert_eq!(f(2.0), 170.0);
    assert_eq!(f(99.0), 100.0);
}
