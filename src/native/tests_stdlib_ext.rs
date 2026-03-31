//! Constraint edge cases, AOT, math/path/process, wait/async, and stdlib memory tests

use cranelift_jit::JITModule;
use cranelift_module::Module;
use crate::native::{create_jit_module, compile_all, compile_to_object, runtime};

macro_rules! mem_test {
    ($name:ident, $body:block) => {
        #[test]
        fn $name() {
            runtime::MEM.reset();
            $body
        }
    };
}

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

// ─── Constraint: empty string edge cases ─────────────

#[test]
fn constraint_empty_string_with_minlen_1() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Name {
            value: String { minLen: 1, default: "x" }
        }{}
        pub fn make() -> Number {
            const n = Name { value: "" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(), "empty string violates minLen: 1");
}

#[test]
fn constraint_contains_empty_needle() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Doc {
            body: String { contains: "", default: "anything" }
        }{}
        pub fn make() -> Number {
            const d = Doc { body: "test" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    assert_eq!(f(), 1.0);
    assert!(!runtime::constraint_violated(), "contains empty string always passes");
}

// ─── Constraint: field with only default ─────────────

#[test]
fn constraint_default_only_no_validation() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Settings {
            timeout: Number { default: 30 }
        }{}
        pub fn make() -> Number {
            const s = Settings { timeout: 999 }
            return s.timeout
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    assert_eq!(f(), 999.0);
    assert!(!runtime::constraint_violated(), "default-only field has no validation");
}

// ─── Constraint: string min/max (treated as minLen/maxLen) ──

#[test]
fn constraint_string_min_as_minlen() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Token {
            code: String { min: 4, default: "abcd" }
        }{}
        pub fn make() -> Number {
            const t = Token { code: "ab" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(), "min on String = minLen, 'ab' (len 2) < 4");
}

#[test]
fn constraint_string_max_as_maxlen() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Token {
            code: String { max: 5, default: "abc" }
        }{}
        pub fn make() -> Number {
            const t = Token { code: "toolongvalue" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(), "max on String = maxLen, 'toolongvalue' (len 12) > 5");
}

// ─── Constraint: multi-field struct ──────────────────

#[test]
fn constraint_multiple_fields_all_valid() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Server {
            port: Number { min: 1, max: 65535, default: 8080 }
            name: String { minLen: 1, maxLen: 32, default: "srv" }
        }{}
        pub fn make() -> Number {
            const s = Server { port: 443, name: "web" }
            return s.port
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    assert_eq!(f(), 443.0);
    assert!(!runtime::constraint_violated(), "all fields valid");
}

#[test]
fn constraint_multiple_fields_second_violated() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Server {
            port: Number { min: 1, max: 65535, default: 8080 }
            name: String { minLen: 1, maxLen: 32, default: "srv" }
        }{}
        pub fn make() -> Number {
            const s = Server { port: 443, name: "" }
            return s.port
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(), "empty name violates minLen: 1");
}

// ─── AOT Test ─────────────────────────────────────────

#[test]
fn aot_produces_object() {
    let file = crate::parse::parse("pub fn add(a: Number, b: Number) -> Number { return a + b }");
    let bytes = compile_to_object(&file).unwrap();
    assert!(bytes.len() > 100, "object file too small: {} bytes", bytes.len());
    assert_eq!(&bytes[1..4], b"ELF", "expected ELF object file");
}

// ─── Math Runtime Tests ─────────────────────────────

#[test]
fn math_functions() {
    assert_eq!(runtime::roca_math_floor(3.7), 3.0);
    assert_eq!(runtime::roca_math_ceil(3.2), 4.0);
    assert_eq!(runtime::roca_math_round(3.5), 4.0);
    assert_eq!(runtime::roca_math_abs(-5.0), 5.0);
    assert_eq!(runtime::roca_math_sqrt(9.0), 3.0);
    assert_eq!(runtime::roca_math_pow(2.0, 10.0), 1024.0);
    assert_eq!(runtime::roca_math_min(3.0, 7.0), 3.0);
    assert_eq!(runtime::roca_math_max(3.0, 7.0), 7.0);
}

#[test]
fn path_join_test() {
    let result = runtime::roca_path_join(b"src\0".as_ptr() as i64, b"main.roca\0".as_ptr() as i64);
    let s = unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap();
    assert_eq!(s, "src/main.roca");
}

#[test]
fn path_dirname_test() {
    let result = runtime::roca_path_dirname(b"src/native/mod.rs\0".as_ptr() as i64);
    let s = unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap();
    assert_eq!(s, "src/native");
}

#[test]
fn path_basename_test() {
    let result = runtime::roca_path_basename(b"src/native/mod.rs\0".as_ptr() as i64);
    let s = unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap();
    assert_eq!(s, "mod.rs");
}

#[test]
fn path_extension_test() {
    let result = runtime::roca_path_extension(b"main.roca\0".as_ptr() as i64);
    let s = unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap();
    assert_eq!(s, ".roca");
}

#[test]
fn process_cwd_test() {
    let result = runtime::roca_process_cwd();
    assert_ne!(result, 0);
    let s = unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap();
    assert!(!s.is_empty());
}

// ─── Wait / Async Tests ────────────────────────────

#[test]
fn wait_single() {
    let mut m = jit(r#"
        pub fn add(a: Number, b: Number) -> Number { return a + b }
        pub fn test_wait() -> Number {
            let result, failed = wait add(3, 4)
            if failed { return 0 }
            return result
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "test_wait", 0)) };
    assert_eq!(f(), 7.0);
}

#[test]
fn wait_expr_await() {
    let mut m = jit(r#"
        pub fn double(n: Number) -> Number { return n * 2 }
        pub fn test_await() -> Number {
            const result = wait double(21)
            return result
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "test_await", 0)) };
    assert_eq!(f(), 42.0);
}

#[test]
fn sleep_timing() {
    let start = std::time::Instant::now();
    runtime::roca_sleep(50.0);
    let elapsed = start.elapsed().as_millis();
    assert!(elapsed >= 45, "sleep too short: {}ms", elapsed);
    assert!(elapsed < 200, "sleep too long: {}ms", elapsed);
}

#[test]
fn time_now_epoch() {
    let now = runtime::roca_time_now();
    assert!(now > 1_700_000_000_000.0, "should be epoch ms, got {}", now);
}

// ─── Memory Tests (stdlib scope) ──────────────────────

mem_test!(rc_alloc_and_release, {
    let ptr = runtime::roca_rc_alloc(32);
    assert_ne!(ptr, 0);
    assert_eq!(runtime::MEM.stats().0, 1);
    assert_eq!(runtime::MEM.stats().1, 0);

    runtime::roca_rc_release(ptr);
    assert_eq!(runtime::MEM.stats().1, 1);
    assert_eq!(runtime::MEM.stats().4, 0);
});

mem_test!(rc_retain_delays_free, {
    let ptr = runtime::roca_rc_alloc(16);
    runtime::roca_rc_retain(ptr); // refcount 2

    runtime::roca_rc_release(ptr); // refcount 1
    assert_eq!(runtime::MEM.stats().1, 0);

    runtime::roca_rc_release(ptr); // refcount 0, freed
    assert_eq!(runtime::MEM.stats().1, 1);
});

mem_test!(rc_null_is_safe, {
    runtime::roca_rc_retain(0);
    runtime::roca_rc_release(0);
    runtime::MEM.assert_clean();
});

mem_test!(rc_multiple_allocs_all_freed, {
    let ptrs: Vec<i64> = (0..10).map(|_| runtime::roca_rc_alloc(24)).collect();
    assert_eq!(runtime::MEM.stats().0, 10);
    for ptr in ptrs { runtime::roca_rc_release(ptr); }
    runtime::MEM.assert_clean();
});

mem_test!(rc_shared_const_pattern, {
    let ptr = runtime::roca_rc_alloc(8);
    runtime::roca_rc_retain(ptr); // refcount 2
    runtime::roca_rc_release(ptr); // refcount 1
    runtime::roca_rc_release(ptr); // refcount 0, freed
    runtime::MEM.assert_clean();
});

mem_test!(mem_scope_frees_string_locals, {
    let mut m = jit(r#"
        pub fn work() -> Number {
            const s = "hello"
            const t = "world"
            return 42
        }
    "#);
    runtime::MEM.reset(); // reset after compilation
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "work", 0)) };
    assert_eq!(f(), 42.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert!(allocs >= 2, "should allocate >= 2 strings, got {}", allocs);
    assert_eq!(allocs, frees, "all string locals freed: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_return_value_not_freed, {
    let mut m = jit(r#"
        pub fn greeting() -> String {
            const extra = "unused"
            return "hello"
        }
    "#);
    let mut sig = m.make_signature();
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("greeting", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn() -> *const u8>(m.get_finalized_function(id)) };
    runtime::MEM.reset();
    let result = f();
    assert!(!result.is_null());
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(frees, allocs - 1, "return value NOT freed: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_struct_freed_at_scope_exit, {
    let mut m = jit(r#"
        pub fn make_point() -> Number {
            const p = Point { x: 10, y: 20 }
            return p.x
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make_point", 0)) };
    runtime::MEM.reset();
    assert_eq!(f(), 10.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert!(allocs >= 1, "should allocate struct");
    assert_eq!(allocs, frees, "struct freed: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_loop_no_leak, {
    let mut m = jit(r#"
        pub fn loop_count() -> Number {
            let i = 0
            while i < 5 {
                const s = "temp"
                i = i + 1
            }
            return i
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "loop_count", 0)) };
    runtime::MEM.reset();
    assert_eq!(f(), 5.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert!(allocs >= 5, "should allocate >= 5 strings, got {}", allocs);
    assert_eq!(allocs, frees, "loop locals freed: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_wait_no_leak, {
    let mut m = jit(r#"
        pub fn make() -> String { return "created" }
        pub fn test_wait_mem() -> Number {
            let result, failed = wait make()
            return result.length
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "test_wait_mem", 0)) };
    assert_eq!(f(), 7.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, frees, "wait no leak: {} allocs, {} frees", allocs, frees);
});
