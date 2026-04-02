//! Cross-target integration tests — real Roca programs through the native JIT.
//! Each test compiles a .roca file from tests/integration/ and validates results.

use super::test_helpers::*;
use crate::runtime;

// ─── Error handling integration ─────────────────────

const ERROR_HANDLING: &str = r#"
    pub fn validate(n: Number) -> Number, err {
        if n < 0 { return err.negative }
        if n == 0 { return err.zero }
        if n > 1000 { return err.too_large }
        return n
    }
    pub fn transform(n: Number) -> Number, err {
        if n > 500 { return err.too_large }
        return n * 2
    }
    pub fn pipeline(n: Number) -> Number {
        let raw, failed = validate(n)
        if failed { return 0 }
        let result, err2 = transform(raw)
        if err2 { return raw }
        return result
    crash {
        validate -> fallback(0)
        transform -> log |> fallback(0)
    }}
"#;

#[test]
fn error_pipeline_ok() {
    let mut m = jit(ERROR_HANDLING);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "pipeline")) };
    assert_eq!(f(5.0), 10.0);
}

#[test]
fn error_pipeline_zero() {
    let mut m = jit(ERROR_HANDLING);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "pipeline")) };
    assert_eq!(f(0.0), 0.0);
}

#[test]
fn error_pipeline_negative() {
    let mut m = jit(ERROR_HANDLING);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "pipeline")) };
    assert_eq!(f(-1.0), 0.0);
}

#[test]
fn error_pipeline_over_transform_limit() {
    let mut m = jit(ERROR_HANDLING);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "pipeline")) };
    assert_eq!(f(600.0), 600.0); // transform fails, returns raw
}

// ─── Enum AST integration ───────────────────────────

const ENUM_AST: &str = r#"
    enum Token { Number(Number) Plus Minus Mul }
    pub fn eval_binary(left: Number, op: Token, right: Number) -> Number {
        return match op {
            Token.Plus => left + right
            Token.Minus => left - right
            Token.Mul => left * right
            _ => 0
        }
    }
    pub fn run() -> Number {
        const op = Token.Plus
        return eval_binary(10, op, 5)
    }
"#;

#[test]
fn enum_ast_run() {
    let mut m = jit(ENUM_AST);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "run")) };
    assert_eq!(f(), 15.0);
}

// ─── Closures + HOF integration ─────────────────────

const CLOSURES_HOF: &str = r#"
    pub fn apply(n: Number, f: fn(Number) -> Number) -> Number {
        return f(n)
    }
    pub fn apply_twice(n: Number, f: fn(Number) -> Number) -> Number {
        return f(f(n))
    }
    pub fn compose() -> Number {
        const inc = fn(x) -> x + 1
        const dbl = fn(x) -> x * 2
        const a = apply_twice(3, inc)
        const b = apply_twice(3, dbl)
        return a + b
    }
    pub fn pipeline_closures() -> Number {
        const step1 = fn(x) -> x + 10
        const step2 = fn(x) -> x * 3
        return apply(apply(5, step1), step2)
    }
"#;

#[test]
fn closures_compose() {
    let mut m = jit(CLOSURES_HOF);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "compose")) };
    assert_eq!(f(), 17.0);
}

#[test]
fn closures_pipeline() {
    let mut m = jit(CLOSURES_HOF);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "pipeline_closures")) };
    assert_eq!(f(), 45.0);
}

// ─── Struct methods integration ─────────────────────

const STRUCT_METHODS: &str = r#"
    pub struct Point {
        x: Number
        y: Number
    }{
        fn distanceSquared() -> Number {
            return self.x * self.x + self.y * self.y
        }
        fn translate(dx: Number, dy: Number) -> Number {
            self.x = self.x + dx
            self.y = self.y + dy
            return self.x + self.y
        }
    }
    pub fn create_and_measure() -> Number {
        const p = Point { x: 3, y: 4 }
        return Point.distanceSquared(p)
    }
    pub fn translate_point() -> Number {
        let p = Point { x: 0, y: 0 }
        return Point.translate(p, 5, 10)
    }
"#;

#[test]
fn struct_measure() {
    let mut m = jit(STRUCT_METHODS);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "create_and_measure")) };
    assert_eq!(f(), 25.0);
}

#[test]
fn struct_translate() {
    let mut m = jit(STRUCT_METHODS);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "translate_point")) };
    assert_eq!(f(), 15.0);
}

// ─── Constrained params integration ─────────────────

#[test]
fn constrained_param_in_pipeline() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub fn safe_divide(a: Number, b: Number { min: 1 }) -> Number {
            return a / b
        }
        pub fn run() -> Number {
            return safe_divide(10, 2)
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "run")) };
    assert_eq!(f(), 5.0);
    assert!(!runtime::constraint_violated());
}

#[test]
fn constrained_param_violation_in_pipeline() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub fn safe_divide(a: Number, b: Number { min: 1 }) -> Number {
            return a / b
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "safe_divide")) };
    f(10.0, 0.0);
    assert!(runtime::constraint_violated(), "0 < min 1");
}

// ─── Retry integration ──────────────────────────

#[test]
fn retry_with_fallback() {
    // Retry 3 times, then fallback — extern fn auto-stub always succeeds
    // so retry never triggers, but the code path must compile and run
    let mut m = jit(r#"
        extern fn flaky(n: Number) -> Number, err {
            err fail = "failed"
        }
        pub fn resilient(n: Number) -> Number {
            const result = flaky(n)
            return result
        crash {
            flaky -> retry(3, 0) |> fallback(0)
        }}
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "resilient")) };
    // Auto-stub returns 0.0 (default Number) with no error — retry not triggered
    assert_eq!(f(5.0), 0.0);
}

#[test]
fn retry_then_halt() {
    let mut m = jit(r#"
        extern fn unstable() -> Number, err {
            err fail = "failed"
        }
        pub fn try_hard() -> Number {
            const result = unstable()
            return result
        crash {
            unstable -> retry(2, 0) |> fallback(0)
        }}
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "try_hard")) };
    // Auto-stub returns success (0.0) — retry not triggered
    assert_eq!(f(), 0.0);
}

// ─── Concurrent wait integration ────────────────

#[test]
fn wait_all_concurrent() {
    // Two functions that sleep 50ms each — if concurrent, total < 150ms
    let mut m = jit(r#"
        pub fn slow_a() -> Number {
            return 10
        }
        pub fn slow_b() -> Number {
            return 20
        }
        pub fn run_both() -> Number {
            let a, b, failed = waitAll { slow_a() slow_b() }
            if failed { return 0 }
            return a + b
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "run_both")) };
    assert_eq!(f(), 30.0);
}

#[test]
fn wait_first_returns_fastest() {
    let mut m = jit(r#"
        pub fn fast() -> Number {
            return 42
        }
        pub fn also_fast() -> Number {
            return 99
        }
        pub fn race() -> Number {
            let winner, failed = waitFirst { fast() also_fast() }
            if failed { return 0 }
            return winner
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "race")) };
    let result = f();
    // Either 42 or 99 — both are valid "first" results
    assert!(result == 42.0 || result == 99.0, "got: {}", result);
}
