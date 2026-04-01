//! Constraint edge cases, AOT, math/path/process, wait/async, and stdlib memory tests

use super::test_helpers::*;
use crate::native::{compile_to_object, runtime};

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
    let s = super::test_helpers::read_native_str(result);
    assert_eq!(s, "src/main.roca");
}

#[test]
fn path_dirname_test() {
    let result = runtime::roca_path_dirname(b"src/native/mod.rs\0".as_ptr() as i64);
    let s = super::test_helpers::read_native_str(result);
    assert_eq!(s, "src/native");
}

#[test]
fn path_basename_test() {
    let result = runtime::roca_path_basename(b"src/native/mod.rs\0".as_ptr() as i64);
    let s = super::test_helpers::read_native_str(result);
    assert_eq!(s, "mod.rs");
}

#[test]
fn path_extension_test() {
    let result = runtime::roca_path_extension(b"main.roca\0".as_ptr() as i64);
    let s = super::test_helpers::read_native_str(result);
    assert_eq!(s, ".roca");
}

#[test]
fn process_cwd_test() {
    let result = runtime::roca_process_cwd();
    assert_ne!(result, 0);
    let s = super::test_helpers::read_native_str(result);
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

// ─── Function parameter constraints ────────────────

#[test]
fn param_constraint_number_valid() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub fn clamp(n: Number { min: 0, max: 100 }) -> Number {
            return n * 2
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "clamp", 1)) };
    assert_eq!(f(50.0), 100.0);
    assert!(!runtime::constraint_violated(), "50 is within 0..100");
}

#[test]
fn param_constraint_number_min_violated() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub fn clamp(n: Number { min: 0, max: 100 }) -> Number {
            return n * 2
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "clamp", 1)) };
    f(-5.0);
    assert!(runtime::constraint_violated(), "-5 < min 0");
}

#[test]
fn param_constraint_number_max_violated() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub fn clamp(n: Number { min: 0, max: 100 }) -> Number {
            return n * 2
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "clamp", 1)) };
    f(200.0);
    assert!(runtime::constraint_violated(), "200 > max 100");
}

#[test]
fn param_constraint_string_contains() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub fn sendEmail(to: String { contains: "@" }) -> Number {
            return to.length
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    let id = m.declare_function("sendEmail", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> f64>(m.get_finalized_function(id)) };

    // Valid — contains @
    runtime::reset_constraint_violated();
    assert_eq!(f(b"user@example.com\0".as_ptr()), 16.0);
    assert!(!runtime::constraint_violated(), "valid email has @");

    // Invalid — missing @
    runtime::reset_constraint_violated();
    f(b"nope\0".as_ptr());
    assert!(runtime::constraint_violated(), "missing @ should violate");
}

#[test]
fn param_constraint_string_minlen() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub fn setName(name: String { minLen: 2, maxLen: 50 }) -> Number {
            return name.length
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    let id = m.declare_function("setName", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> f64>(m.get_finalized_function(id)) };

    // Valid
    runtime::reset_constraint_violated();
    assert_eq!(f(b"Cameron\0".as_ptr()), 7.0);
    assert!(!runtime::constraint_violated());

    // Too short
    runtime::reset_constraint_violated();
    f(b"X\0".as_ptr());
    assert!(runtime::constraint_violated(), "1 char < minLen 2");
}

#[test]
fn param_constraint_multiple_params() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub fn createUser(age: Number { min: 0, max: 150 }, score: Number { min: 0, max: 1000 }) -> Number {
            return age + score
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "createUser", 2)) };

    // Both valid
    runtime::reset_constraint_violated();
    assert_eq!(f(25.0, 500.0), 525.0);
    assert!(!runtime::constraint_violated());

    // First param violated
    runtime::reset_constraint_violated();
    f(-1.0, 500.0);
    assert!(runtime::constraint_violated(), "age -1 < min 0");

    // Second param violated
    runtime::reset_constraint_violated();
    f(25.0, 2000.0);
    assert!(runtime::constraint_violated(), "score 2000 > max 1000");
}

// ─── Crypto ─────────────────────────────────────────

#[test]
fn crypto_random_uuid_length() {
    let uuid = runtime::roca_crypto_random_uuid();
    let s = super::test_helpers::read_native_str(uuid);
    assert_eq!(s.len(), 36, "UUID should be 36 chars: {}", s);
}

#[test]
fn crypto_uuid_unique() {
    let a = runtime::roca_crypto_random_uuid();
    let b = runtime::roca_crypto_random_uuid();
    let a_str = super::test_helpers::read_native_str(a);
    let b_str = super::test_helpers::read_native_str(b);
    assert_ne!(a_str, b_str, "two UUIDs should be different");
}

#[test]
fn crypto_sha256_known_hash() {
    let input = runtime::alloc_str("");
    let hash = runtime::roca_crypto_sha256(input);
    let s = super::test_helpers::read_native_str(hash);
    assert!(s.starts_with("e3b0c44"), "SHA-256 of empty string: {}", s);
    assert_eq!(s.len(), 64, "SHA-256 hex should be 64 chars");
}

#[test]
fn crypto_sha512_known_hash() {
    let input = runtime::alloc_str("");
    let hash = runtime::roca_crypto_sha512(input);
    let s = super::test_helpers::read_native_str(hash);
    assert!(s.starts_with("cf83e1"), "SHA-512 of empty string: {}", s);
    assert_eq!(s.len(), 128, "SHA-512 hex should be 128 chars");
}

// ─── Url ────────────────────────────────────────────

#[test]
fn url_parse_valid() {
    let raw = runtime::alloc_str("https://example.com:8080/path?q=1#frag");
    let (ptr, err) = runtime::roca_url_parse(raw);
    assert_eq!(err, 0, "parse should succeed");
    assert_ne!(ptr, 0);

    let hostname = runtime::roca_url_hostname(ptr);
    let h = super::test_helpers::read_native_str(hostname);
    assert_eq!(h, "example.com");

    let protocol = runtime::roca_url_protocol(ptr);
    let p = super::test_helpers::read_native_str(protocol);
    assert_eq!(p, "https:");

    let pathname = runtime::roca_url_pathname(ptr);
    let pa = super::test_helpers::read_native_str(pathname);
    assert_eq!(pa, "/path");

    let search = runtime::roca_url_search(ptr);
    let s = super::test_helpers::read_native_str(search);
    assert_eq!(s, "?q=1");

    let hash = runtime::roca_url_hash(ptr);
    let f = super::test_helpers::read_native_str(hash);
    assert_eq!(f, "#frag");
}

#[test]
fn url_parse_invalid() {
    let raw = runtime::alloc_str("not a url");
    let (_, err) = runtime::roca_url_parse(raw);
    assert_ne!(err, 0, "parse should fail for invalid URL");
}

#[test]
fn url_is_valid_check() {
    let valid = runtime::alloc_str("https://example.com");
    assert_eq!(runtime::roca_url_is_valid(valid), 1);

    let invalid = runtime::alloc_str("nope");
    assert_eq!(runtime::roca_url_is_valid(invalid), 0);
}

#[test]
fn url_get_param_works() {
    let raw = runtime::alloc_str("https://x.com?foo=bar&baz=42");
    let (ptr, _) = runtime::roca_url_parse(raw);
    let key = runtime::alloc_str("foo");
    let val = runtime::roca_url_get_param(ptr, key);
    let v = super::test_helpers::read_native_str(val);
    assert_eq!(v, "bar");

    let missing = runtime::alloc_str("nope");
    assert_eq!(runtime::roca_url_get_param(ptr, missing), 0);
}

#[test]
fn url_has_param_works() {
    let raw = runtime::alloc_str("https://x.com?foo=bar");
    let (ptr, _) = runtime::roca_url_parse(raw);
    let key = runtime::alloc_str("foo");
    assert_eq!(runtime::roca_url_has_param(ptr, key), 1);
    let missing = runtime::alloc_str("nope");
    assert_eq!(runtime::roca_url_has_param(ptr, missing), 0);
}

// ─── Encoding ───────────────────────────────────────

#[test]
fn encoding_btoa_hello() {
    let input = runtime::alloc_str("hello");
    let (result, err) = runtime::roca_encoding_btoa(input);
    assert_eq!(err, 0);
    let s = super::test_helpers::read_native_str(result);
    assert_eq!(s, "aGVsbG8=");
}

#[test]
fn encoding_atob_hello() {
    let input = runtime::alloc_str("aGVsbG8=");
    let (result, err) = runtime::roca_encoding_atob(input);
    assert_eq!(err, 0);
    let s = super::test_helpers::read_native_str(result);
    assert_eq!(s, "hello");
}

#[test]
fn encoding_roundtrip() {
    let input = runtime::alloc_str("test data 123");
    let (encoded, _) = runtime::roca_encoding_btoa(input);
    let (decoded, _) = runtime::roca_encoding_atob(encoded);
    let s = super::test_helpers::read_native_str(decoded);
    assert_eq!(s, "test data 123");
}

#[test]
fn encoding_atob_invalid() {
    let input = runtime::alloc_str("!!!invalid!!!");
    let (_, err) = runtime::roca_encoding_atob(input);
    assert_ne!(err, 0, "invalid base64 should return error");
}

// ─── JSON ───────────────────────────────────────────

#[test]
fn json_parse_valid() {
    let text = runtime::alloc_str(r#"{"name":"cam","age":30,"active":true}"#);
    let (ptr, err) = runtime::roca_json_parse(text);
    assert_eq!(err, 0);
    assert_ne!(ptr, 0);

    let name_key = runtime::alloc_str("name");
    let name = runtime::roca_json_get_string(ptr, name_key);
    let n = super::test_helpers::read_native_str(name);
    assert_eq!(n, "cam");

    let age_key = runtime::alloc_str("age");
    let age = runtime::roca_json_get_number(ptr, age_key);
    assert_eq!(age, 30.0);

    let active_key = runtime::alloc_str("active");
    let active = runtime::roca_json_get_bool(ptr, active_key);
    assert_eq!(active, 1);
}

#[test]
fn json_parse_invalid() {
    let text = runtime::alloc_str("not json");
    let (_, err) = runtime::roca_json_parse(text);
    assert_ne!(err, 0);
}

#[test]
fn json_stringify_roundtrip() {
    let text = runtime::alloc_str(r#"{"a":1}"#);
    let (ptr, _) = runtime::roca_json_parse(text);
    let output = runtime::roca_json_stringify(ptr);
    let s = super::test_helpers::read_native_str(output);
    assert!(s.contains("\"a\""), "should contain key: {}", s);
    assert!(s.contains("1"), "should contain value: {}", s);
}

#[test]
fn json_nested_get() {
    let text = runtime::alloc_str(r#"{"user":{"name":"cam"}}"#);
    let (ptr, _) = runtime::roca_json_parse(text);
    let user_key = runtime::alloc_str("user");
    let user = runtime::roca_json_get(ptr, user_key);
    assert_ne!(user, 0);
    let name_key = runtime::alloc_str("name");
    let name = runtime::roca_json_get_string(user, name_key);
    let n = super::test_helpers::read_native_str(name);
    assert_eq!(n, "cam");
}

// ─── Http ───────────────────────────────────────────

#[test]
#[ignore] // requires network
fn http_get_real() {
    let url = runtime::alloc_str("https://httpbin.org/get");
    let (resp, err) = runtime::roca_http_get(url);
    assert_eq!(err, 0, "GET should succeed");
    let status = runtime::roca_http_status(resp);
    assert_eq!(status, 200.0);
    assert_eq!(runtime::roca_http_ok(resp), 1);
}

#[test]
fn http_get_bad_url() {
    let url = runtime::alloc_str("http://localhost:1");
    let (_, err) = runtime::roca_http_get(url);
    assert_ne!(err, 0, "bad URL should return error");
}
