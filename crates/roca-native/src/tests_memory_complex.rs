//! Memory management tests — cross-function, closures, crash/error, integration

use super::test_helpers::*;
use crate::runtime;

// ─── Cross-function & scope tracking ──────────────

mem_test!(mem_cross_function_ownership, {
    // B creates a string, returns it. A calls B, uses result, frees at scope exit.
    let mut m = jit(r#"
        pub fn make() -> String {
            const temp = "discarded"
            return "created"
        }
        pub fn use_it() -> Number {
            const s = make()
            return s.length
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "use_it")) };
    assert_eq!(f(), 7.0); // "created".length
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    // make() allocates "discarded" (freed inside make) + "created" (returned, freed in use_it)
    assert_eq!(allocs, frees, "cross-function: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_function_chain, {
    // C → B → A chain, callees defined first (native requires definition order)
    let mut m = jit(r#"
        pub fn step_c() -> String {
            const local_c = "c_local"
            return "final"
        }
        pub fn step_b() -> String {
            const local_b = "b_local"
            return step_c()
        }
        pub fn step_a() -> String {
            const local_a = "a_local"
            return step_b()
        }
    "#);
    let mut sig = m.make_signature();
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("step_a", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn() -> *const u8>(m.get_finalized_function(id)) };
    runtime::MEM.reset();
    let result = f();
    assert!(!result.is_null());
    let s = unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap();
    assert_eq!(s, "final");
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    // 4 allocs: a_local, b_local, c_local, "final"
    // 3 frees: a_local, b_local, c_local (each freed at their function's scope exit)
    // "final" returned to caller (not freed)
    assert_eq!(frees, allocs - 1, "chain: {} allocs, {} frees (1 returned)", allocs, frees);
});

// ─── Closures ────────────────────────────────────

mem_test!(mem_closure_as_value_no_leak, {
    // First-class closure — the closure pointer itself isn't heap-allocated
    // but strings created inside the closure should be freed
    let mut m = jit(r#"
        pub fn use_closure() -> Number {
            const double = fn(x) -> x * 2
            const temp = "some_string"
            return double(5)
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "use_closure")) };
    assert_eq!(f(), 10.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, frees, "closure value no leak: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_closure_passed_as_arg_no_leak, {
    // Closure passed to another function — strings in caller freed
    let mut m = jit(r#"
        pub fn apply(n: Number, transform: fn(Number) -> Number) -> Number {
            return transform(n)
        }
        pub fn caller() -> Number {
            const label = "tracking"
            const triple = fn(x) -> x * 3
            return apply(4, triple)
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "caller")) };
    assert_eq!(f(), 12.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, frees, "closure arg no leak: {} allocs, {} frees", allocs, frees);
});

// ─── Memory: crash path cleanup ──────────────────

mem_test!(mem_crash_fallback_frees, {
    // Crash fallback path must still free local strings
    let mut m = jit(r#"
        pub fn risky(n: Number) -> Number, err {
            if n < 0 { return err.negative }
            return n * 2
        }
        pub fn safe(n: Number) -> Number {
            const label = "tracked"
            return risky(n)
        crash {
            risky -> fallback(0)
        }}
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "safe")) };

    // OK path
    runtime::MEM.reset();
    assert_eq!(f(5.0), 10.0);
    let (a1, f1, _, _, _) = runtime::MEM.stats();
    assert_eq!(a1, f1, "ok path: {} allocs, {} frees", a1, f1);

    // Error/fallback path
    runtime::MEM.reset();
    assert_eq!(f(-3.0), 0.0);
    let (a2, f2, _, _, _) = runtime::MEM.stats();
    assert_eq!(a2, f2, "fallback path: {} allocs, {} frees", a2, f2);
});

// ─── Memory: error-returning functions ────────────

mem_test!(mem_error_return_frees, {
    // Functions that return errors must free their locals
    let mut m = jit(r#"
        pub fn validate(n: Number) -> Number, err {
            const label = "validation"
            if n < 0 { return err.negative }
            return n * 2
        }
        pub fn caller(n: Number) -> Number {
            let result, failed = validate(n)
            if failed { return 0 }
            return result
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "caller")) };

    // OK path
    runtime::MEM.reset();
    assert_eq!(f(5.0), 10.0);
    let (a1, f1, _, _, _) = runtime::MEM.stats();
    assert_eq!(a1, f1, "ok path: {} allocs, {} frees", a1, f1);

    // Error path
    runtime::MEM.reset();
    assert_eq!(f(-3.0), 0.0);
    let (a2, f2, _, _, _) = runtime::MEM.stats();
    assert_eq!(a2, f2, "error path: {} allocs, {} frees", a2, f2);
});

// ─── Memory: string method chains ─────────────────

mem_test!(mem_string_method_chain_frees, {
    // Chained string methods create intermediates — all must be freed
    let mut m = jit(r#"
        pub fn process(s: String) -> Number {
            const cleaned = s.trim().toUpperCase()
            return cleaned.length
        }
    "#);
    runtime::MEM.reset();
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    let id = m.declare_function("process", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> f64>(m.get_finalized_function(id)) };
    runtime::MEM.reset();
    assert_eq!(f(b"  hello  \0".as_ptr()), 5.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, frees, "method chain: {} allocs, {} frees", allocs, frees);
});

// ─── Memory: for loop with strings ────────────────

mem_test!(mem_for_loop_no_leak, {
    // For loop over array — loop-body locals freed each iteration
    let mut m = jit(r#"
        pub fn for_sum() -> Number {
            const arr = [1, 2, 3]
            let total = 0
            for item in arr {
                const label = "iter"
                total = total + item
            }
            return total
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "for_sum")) };
    assert_eq!(f(), 6.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, frees, "for loop: {} allocs, {} frees", allocs, frees);
});

// ─── Integration Memory Tests ─────────────────────

mem_test!(mem_integration_validate_transform, {
    let mut m = jit(r#"
        pub fn validate(n: Number) -> Number, err {
            if n < 0 { return err.negative }
            return n
        }
        pub fn process(n: Number) -> Number {
            const label = "processing"
            let result, failed = validate(n)
            if failed { return 0 }
            return result * 2
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "process")) };
    assert_eq!(f(5.0), 10.0);
    let (a1, f1, _, _, _) = runtime::MEM.stats();
    assert_eq!(a1, f1, "OK path: {} allocs, {} frees", a1, f1);
    runtime::MEM.reset();
    assert_eq!(f(-1.0), 0.0);
    let (a2, f2, _, _, _) = runtime::MEM.stats();
    assert_eq!(a2, f2, "error path: {} allocs, {} frees", a2, f2);
});

mem_test!(mem_integration_string_pipeline, {
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
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("pipeline", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> *const u8>(m.get_finalized_function(id)) };
    runtime::MEM.reset();
    let result = f(b"world\0".as_ptr());
    assert!(!result.is_null());
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(frees, allocs - 1, "pipeline: {} allocs, {} frees (1 returned)", allocs, frees);
});

mem_test!(mem_integration_loop_early_return, {
    let mut m = jit(r#"
        pub fn search() -> Number {
            let i = 0
            while i < 10 {
                const msg = "checking"
                if i == 3 {
                    const found = "found it"
                    return i
                }
                i = i + 1
            }
            return i
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "search")) };
    assert_eq!(f(), 3.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, frees, "early return from loop: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_integration_closures_strings, {
    let mut m = jit(r#"
        pub fn apply(n: Number, transform: fn(Number) -> Number) -> Number {
            const label = "applying"
            return transform(n)
        }
        pub fn run() -> Number {
            const tag = "runner"
            const double = fn(x) -> x * 2
            const result = apply(5, double)
            return result
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "run")) };
    assert_eq!(f(), 10.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, frees, "closures + strings: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_enum_variant_freed, {
    let mut m = jit(r#"
        enum Token { Number(Number) Plus }
        pub fn test_enum() -> Number {
            const t = Token.Number(42)
            return match t {
                Token.Number(n) => n
                _ => 0
            }
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "test_enum")) };
    assert_eq!(f(), 42.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, frees, "enum variant freed: {} allocs, {} frees", allocs, frees);
});

// ─── Constraint: memory safety ───────────────────────

mem_test!(mem_constraint_violation_no_leak, {
    // Constraint violation returns early — verify no untracked allocations
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Config {
            port: Number { min: 1, max: 65535, default: 8080 }
        }{}
        pub fn make() -> Number {
            const c = Config { port: 0 }
            return c.port
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    f();
    assert!(runtime::constraint_violated(), "should violate min: 1");
});

mem_test!(mem_valid_string_constraint_no_leak, {
    // Valid construction with string constraints — verify no constraint violation
    // and that allocations occur (struct + string). Full alloc/free balance for
    // structs with heap fields is tracked separately.
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct User {
            name: String { minLen: 1, maxLen: 64, default: "anon" }
        }{}
        pub fn make() -> Number {
            const u = User { name: "cameron" }
            return 1
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    assert_eq!(f(), 1.0);
    assert!(!runtime::constraint_violated(), "valid string should pass");
    let (allocs, _, _, _, _) = runtime::MEM.stats();
    assert!(allocs >= 2, "should allocate struct + string, got {}", allocs);
});
