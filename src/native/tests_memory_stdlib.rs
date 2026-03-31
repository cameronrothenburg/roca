//! Memory tests for self-hosting stdlib: Char, NumberParse, Map

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

/// Create an RC-managed string from a static literal for testing
fn test_str(s: &str) -> i64 {
    let leaked = Box::leak(format!("{}\0", s).into_boxed_str());
    runtime::roca_string_new(leaked.as_ptr() as i64)
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

// ─── charCodeAt memory ──────────────────────────

mem_test!(mem_char_code_at_no_leak, {
    let mut m = jit(r#"
        pub fn get_code() -> Number {
            const s = "hello"
            return s.charCodeAt(0)
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "get_code", 0)) };
    assert_eq!(f(), 104.0); // 'h' = 104
    let (a, fr, _, _, _) = runtime::MEM.stats();
    assert_eq!(a, fr, "charCodeAt: {} allocs, {} frees", a, fr);
});

mem_test!(mem_char_code_at_in_loop, {
    let mut m = jit(r#"
        pub fn sum_codes() -> Number {
            const s = "abc"
            let total = 0
            let i = 0
            while i < 3 {
                total = total + s.charCodeAt(i)
                i = i + 1
            }
            return total
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "sum_codes", 0)) };
    assert_eq!(f(), 294.0); // 97 + 98 + 99
    let (a, fr, _, _, _) = runtime::MEM.stats();
    assert_eq!(a, fr, "charCodeAt loop: {} allocs, {} frees", a, fr);
});

mem_test!(mem_string_chain_char_code, {
    let mut m = jit(r#"
        pub fn upper_code() -> Number {
            const s = "hello"
            const upper = s.toUpperCase()
            return upper.charCodeAt(0)
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "upper_code", 0)) };
    assert_eq!(f(), 72.0); // 'H' = 72
    let (a, fr, _, _, _) = runtime::MEM.stats();
    assert_eq!(a, fr, "chain + charCodeAt: {} allocs, {} frees", a, fr);
});

// ─── Map memory ─────────────────────────────────

#[test]
fn map_lifecycle_no_leak() {
    runtime::MEM.reset();
    let map = runtime::roca_map_new();
    assert!(map != 0);

    let key = test_str("name");
    let val = test_str("roca");

    runtime::roca_map_set(map, key, val);
    assert_eq!(runtime::roca_map_has(map, key), 1);
    assert_eq!(runtime::roca_map_size(map), 1.0);

    let got = runtime::roca_map_get(map, key);
    assert_eq!(got, val);

    runtime::roca_map_delete(map, key);
    assert_eq!(runtime::roca_map_has(map, key), 0);

    runtime::roca_rc_release(key);
    runtime::roca_rc_release(val);
    runtime::roca_map_free(map);

    let (a, fr, _, _, _) = runtime::MEM.stats();
    assert_eq!(a, fr, "map lifecycle: {} allocs, {} frees", a, fr);
}

#[test]
fn map_keys_returns_array() {
    runtime::MEM.reset();
    let map = runtime::roca_map_new();
    let k1 = test_str("a");
    let k2 = test_str("b");

    runtime::roca_map_set(map, k1, 42);
    runtime::roca_map_set(map, k2, 99);

    let keys = runtime::roca_map_keys(map);
    assert!(keys != 0);

    runtime::roca_rc_release(k1);
    runtime::roca_rc_release(k2);
    runtime::roca_map_free(map);
}

#[test]
fn map_null_guards() {
    // All map functions handle null (0) gracefully
    assert_eq!(runtime::roca_map_get(0, 0), 0);
    assert_eq!(runtime::roca_map_has(0, 0), 0);
    assert_eq!(runtime::roca_map_size(0), 0.0);
    assert_eq!(runtime::roca_map_keys(0), 0);
    assert_eq!(runtime::roca_map_values(0), 0);
    runtime::roca_map_free(0); // should not crash
}

// ─── NumberParse ────────────────────────────────

#[test]
fn number_parse_valid() {
    let s = test_str("42");
    let result = runtime::roca_number_parse(s);
    assert_eq!(result, 42.0);
    runtime::roca_rc_release(s);
}

#[test]
fn number_parse_invalid_returns_nan() {
    let s = test_str("abc");
    let result = runtime::roca_number_parse(s);
    assert!(result.is_nan());
    runtime::roca_rc_release(s);
}

#[test]
fn number_parse_float() {
    let s = test_str("3.14");
    let result = runtime::roca_number_parse(s);
    assert!((result - 3.14).abs() < 0.001);
    runtime::roca_rc_release(s);
}

// ─── Char classification ────────────────────────

#[test]
fn char_is_digit() {
    let d = test_str("5");
    assert_eq!(runtime::roca_char_is_digit(d), 1);
    runtime::roca_rc_release(d);

    let a = test_str("a");
    assert_eq!(runtime::roca_char_is_digit(a), 0);
    runtime::roca_rc_release(a);
}

#[test]
fn char_is_letter() {
    let z = test_str("Z");
    assert_eq!(runtime::roca_char_is_letter(z), 1);
    runtime::roca_rc_release(z);

    let n = test_str("9");
    assert_eq!(runtime::roca_char_is_letter(n), 0);
    runtime::roca_rc_release(n);
}

#[test]
fn char_is_whitespace() {
    let sp = test_str(" ");
    assert_eq!(runtime::roca_char_is_whitespace(sp), 1);
    runtime::roca_rc_release(sp);

    let x = test_str("x");
    assert_eq!(runtime::roca_char_is_whitespace(x), 0);
    runtime::roca_rc_release(x);
}

#[test]
fn char_from_code() {
    let result = runtime::roca_char_from_code(65.0);
    let s = unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap();
    assert_eq!(s, "A");
    runtime::roca_rc_release(result);
}

#[test]
fn char_code_at() {
    let s = test_str("ABC");
    assert_eq!(runtime::roca_string_char_code_at(s, 0), 65.0);
    assert_eq!(runtime::roca_string_char_code_at(s, 1), 66.0);
    assert_eq!(runtime::roca_string_char_code_at(s, 2), 67.0);
    assert!(runtime::roca_string_char_code_at(s, 99).is_nan());
    runtime::roca_rc_release(s);
}
