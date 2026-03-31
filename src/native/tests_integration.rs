//! Cross-target integration tests — real Roca programs through the native JIT.
//! Each test compiles a .roca file from tests/integration/ and validates results.

use cranelift_jit::JITModule;
use cranelift_module::Module;
use crate::native::{create_jit_module, compile_all, runtime};

fn jit(source: &str) -> JITModule {
    let file = crate::parse::parse(source);
    let mut module = create_jit_module();
    compile_all(&mut module, &file).unwrap();
    module.finalize_definitions().unwrap();
    module
}

fn sig_f64(m: &JITModule, params: usize) -> cranelift_codegen::ir::Signature {
    let mut s = m.make_signature();
    for _ in 0..params { s.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64)); }
    s.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    s
}

unsafe fn call_f64(m: &mut JITModule, name: &str, params: usize) -> *const u8 {
    let sig = sig_f64(m, params);
    let id = m.declare_function(name, cranelift_module::Linkage::Export, &sig).unwrap();
    m.get_finalized_function(id)
}

macro_rules! mem_test {
    ($name:ident, $body:block) => {
        #[test]
        fn $name() {
            runtime::MEM.reset();
            $body
        }
    };
}

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
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "pipeline", 1)) };
    assert_eq!(f(5.0), 10.0);
}

#[test]
fn error_pipeline_zero() {
    let mut m = jit(ERROR_HANDLING);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "pipeline", 1)) };
    assert_eq!(f(0.0), 0.0);
}

#[test]
fn error_pipeline_negative() {
    let mut m = jit(ERROR_HANDLING);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "pipeline", 1)) };
    assert_eq!(f(-1.0), 0.0);
}

#[test]
fn error_pipeline_over_transform_limit() {
    let mut m = jit(ERROR_HANDLING);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "pipeline", 1)) };
    assert_eq!(f(600.0), 600.0); // transform fails, returns raw
}

mem_test!(error_pipeline_no_leak, {
    let mut m = jit(ERROR_HANDLING);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "pipeline", 1)) };
    runtime::MEM.reset();
    f(5.0);
    let (a, fr, _, _, _) = runtime::MEM.stats();
    assert_eq!(a, fr, "ok path: {} allocs, {} frees", a, fr);
    runtime::MEM.reset();
    f(-1.0);
    let (a2, f2, _, _, _) = runtime::MEM.stats();
    assert_eq!(a2, f2, "error path: {} allocs, {} frees", a2, f2);
});

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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "run", 0)) };
    assert_eq!(f(), 15.0);
}

mem_test!(enum_ast_no_leak, {
    let mut m = jit(ENUM_AST);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "run", 0)) };
    runtime::MEM.reset();
    f();
    let (a, fr, _, _, _) = runtime::MEM.stats();
    assert_eq!(a, fr, "enum ast: {} allocs, {} frees", a, fr);
});

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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "compose", 0)) };
    assert_eq!(f(), 17.0);
}

#[test]
fn closures_pipeline() {
    let mut m = jit(CLOSURES_HOF);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "pipeline_closures", 0)) };
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "create_and_measure", 0)) };
    assert_eq!(f(), 25.0);
}

#[test]
fn struct_translate() {
    let mut m = jit(STRUCT_METHODS);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "translate_point", 0)) };
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "run", 0)) };
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
    let f = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "safe_divide", 2)) };
    f(10.0, 0.0);
    assert!(runtime::constraint_violated(), "0 < min 1");
}
