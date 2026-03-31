//! IO, constraints, async, ownership tests

use cranelift_jit::JITModule;
use cranelift_module::Module;
use crate::native::{create_jit_module, compile_all, compile_to_object, runtime, test_runner};

macro_rules! mem_test {
    ($name:ident, $body:block) => {
        #[test]
        fn $name() { runtime::MEM.reset(); $body }
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
// ─── File I/O Tests ────────────────────────────────

#[test]
fn fs_read_file() {
    let tmp = std::env::temp_dir().join("roca_test_read.txt");
    std::fs::write(&tmp, "hello roca").unwrap();
    let path_cstr = format!("{}\0", tmp.display());
    let (ptr, err) = runtime::roca_fs_read_file(path_cstr.as_ptr() as i64);
    assert_eq!(err, 0, "should succeed");
    assert_ne!(ptr, 0);
    let content = unsafe { std::ffi::CStr::from_ptr(ptr as *const i8) }.to_str().unwrap();
    assert_eq!(content, "hello roca");
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn fs_read_file_not_found() {
    let path = "/tmp/roca_nonexistent_file_12345.txt\0";
    let (_, err) = runtime::roca_fs_read_file(path.as_ptr() as i64);
    assert_eq!(err, 1, "should return not_found error tag");
}

#[test]
fn fs_write_file() {
    let tmp = std::env::temp_dir().join("roca_test_write.txt");
    let path_cstr = format!("{}\0", tmp.display());
    let content = "written by roca\0";
    let err = runtime::roca_fs_write_file(
        path_cstr.as_ptr() as i64,
        content.as_ptr() as i64,
    );
    assert_eq!(err, 0, "should succeed");
    let read_back = std::fs::read_to_string(&tmp).unwrap();
    assert_eq!(read_back, "written by roca");
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn fs_exists() {
    let tmp = std::env::temp_dir().join("roca_test_exists.txt");
    std::fs::write(&tmp, "x").unwrap();
    let path_cstr = format!("{}\0", tmp.display());
    assert_eq!(runtime::roca_fs_exists(path_cstr.as_ptr() as i64), 1);
    std::fs::remove_file(&tmp).ok();
    assert_eq!(runtime::roca_fs_exists(path_cstr.as_ptr() as i64), 0);
}

#[test]
fn fs_read_dir() {
    let tmp_dir = std::env::temp_dir().join("roca_test_dir");
    std::fs::create_dir_all(&tmp_dir).ok();
    std::fs::write(tmp_dir.join("a.txt"), "a").ok();
    std::fs::write(tmp_dir.join("b.txt"), "b").ok();
    let path_cstr = format!("{}\0", tmp_dir.display());
    let (arr_ptr, err) = runtime::roca_fs_read_dir(path_cstr.as_ptr() as i64);
    assert_eq!(err, 0, "should succeed");
    assert_ne!(arr_ptr, 0);
    let len = runtime::roca_array_len(arr_ptr);
    assert!(len >= 2, "should have at least 2 entries, got {}", len);
    std::fs::remove_dir_all(&tmp_dir).ok();
}

#[test]
fn fs_read_dir_not_found() {
    let path = "/tmp/roca_nonexistent_dir_12345\0";
    let (_, err) = runtime::roca_fs_read_dir(path.as_ptr() as i64);
    assert_eq!(err, 1, "should return not_found");
}

// ─── Constraint Validation Tests ──────────────────

#[test]
fn constraint_number_valid() {
    // Valid number within constraints — should not trap
    let mut m = jit(r#"
        pub struct Config {
            port: Number { min: 1, max: 65535, default: 8080 }
        }{}
        pub fn make() -> Number {
            const c = Config { port: 8080 }
            return c.port
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    assert_eq!(f(), 8080.0);
}

#[test]
fn constraint_string_minlen_valid() {
    // Valid string within length constraint
    let mut m = jit(r#"
        pub struct User {
            name: String { minLen: 1, maxLen: 64, default: "anon" }
        }{}
        pub fn make() -> Number {
            const u = User { name: "cameron" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    assert_eq!(f(), 1.0);
}

#[test]
fn constraint_contains_valid() {
    // String contains "@" — valid
    let mut m = jit(r#"
        pub struct Email {
            value: String { contains: "@", default: "a@b.com" }
        }{}
        pub fn make() -> Number {
            const e = Email { value: "test@example.com" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    assert_eq!(f(), 1.0);
}

#[test]
fn constraint_number_min_violated() {
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(),
        "port: 0 should violate min: 1");
}

#[test]
fn constraint_number_max_violated() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Config {
            port: Number { min: 1, max: 65535, default: 8080 }
        }{}
        pub fn make() -> Number {
            const c = Config { port: 99999 }
            return c.port
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(),
        "port: 99999 should violate max: 65535");
}

#[test]
fn constraint_contains_violated() {
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Email {
            value: String { contains: "@", default: "a@b.com" }
        }{}
        pub fn make() -> Number {
            const e = Email { value: "no-at-sign" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(),
        "value without @ should violate contains: @");
}

// ─── Constraint: multiple constraints on same field ──

#[test]
fn constraint_number_min_and_max_valid() {
    // Value within both min and max — should pass
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Range {
            value: Number { min: 1, max: 100, default: 50 }
        }{}
        pub fn make() -> Number {
            const r = Range { value: 50 }
            return r.value
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    assert_eq!(f(), 50.0);
    assert!(!runtime::constraint_violated(), "50 is within [1, 100]");
}

#[test]
fn constraint_number_min_and_max_below() {
    // Value below min with both min and max — should fail
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Range {
            value: Number { min: 1, max: 100, default: 50 }
        }{}
        pub fn make() -> Number {
            const r = Range { value: 0 }
            return r.value
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(), "0 violates min: 1");
}

#[test]
fn constraint_number_min_and_max_above() {
    // Value above max with both min and max — should fail
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Range {
            value: Number { min: 1, max: 100, default: 50 }
        }{}
        pub fn make() -> Number {
            const r = Range { value: 200 }
            return r.value
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(), "200 violates max: 100");
}

#[test]
fn constraint_string_minlen_and_maxlen_valid() {
    // String length within both bounds
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Tag {
            label: String { minLen: 3, maxLen: 10, default: "abc" }
        }{}
        pub fn make() -> Number {
            const t = Tag { label: "hello" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    assert_eq!(f(), 1.0);
    assert!(!runtime::constraint_violated(), "'hello' (len 5) is within [3, 10]");
}

#[test]
fn constraint_string_minlen_violated() {
    // String too short
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Tag {
            label: String { minLen: 3, maxLen: 10, default: "abc" }
        }{}
        pub fn make() -> Number {
            const t = Tag { label: "ab" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(), "'ab' (len 2) violates minLen: 3");
}

#[test]
fn constraint_string_maxlen_violated() {
    // String too long
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Tag {
            label: String { minLen: 3, maxLen: 10, default: "abc" }
        }{}
        pub fn make() -> Number {
            const t = Tag { label: "this is way too long" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(), "'this is way too long' violates maxLen: 10");
}

// ─── Constraint: boundary values ─────────────────────

#[test]
fn constraint_number_at_exact_min() {
    // Value == min exactly — should pass
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Bound {
            value: Number { min: 5, default: 5 }
        }{}
        pub fn make() -> Number {
            const b = Bound { value: 5 }
            return b.value
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    assert_eq!(f(), 5.0);
    assert!(!runtime::constraint_violated(), "5 == min 5, should pass");
}

#[test]
fn constraint_number_at_exact_max() {
    // Value == max exactly — should pass
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Bound {
            value: Number { max: 100, default: 50 }
        }{}
        pub fn make() -> Number {
            const b = Bound { value: 100 }
            return b.value
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    assert_eq!(f(), 100.0);
    assert!(!runtime::constraint_violated(), "100 == max 100, should pass");
}

#[test]
fn constraint_number_just_below_min() {
    // Value just below min — should fail
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Bound {
            value: Number { min: 5, default: 5 }
        }{}
        pub fn make() -> Number {
            const b = Bound { value: 4 }
            return b.value
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(), "4 < min 5, should fail");
}

#[test]
fn constraint_number_just_above_max() {
    // Value just above max — should fail
    runtime::reset_constraint_violated();
    let mut m = jit(r#"
        pub struct Bound {
            value: Number { max: 100, default: 50 }
        }{}
        pub fn make() -> Number {
            const b = Bound { value: 101 }
            return b.value
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make", 0)) };
    f();
    assert!(runtime::constraint_violated(), "101 > max 100, should fail");
}

// ─── Constraint: empty string edge cases ─────────────

#[test]
fn constraint_empty_string_with_minlen_1() {
    // Empty string with minLen: 1 — should fail
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
    // Contains with empty search string — should always pass
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
    // Field with only a default constraint — no validation needed, should pass
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
    // min on a String field is treated as minLen
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
    // max on a String field is treated as maxLen
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
    // Struct with constraints on multiple fields — all valid
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
    // First field valid, second field violated
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

#[test]
fn aot_produces_object() {
    let file = crate::parse::parse("pub fn add(a: Number, b: Number) -> Number { return a + b }");
    let bytes = compile_to_object(&file).unwrap();
    assert!(bytes.len() > 100, "object file too small: {} bytes", bytes.len());
    assert_eq!(&bytes[1..4], b"ELF", "expected ELF object file");
}

// ─── Memory Tests ──────────────────────────────────
// Thread-local counters — no lock needed, tests run in parallel safely.
// Pattern: reset → compile → run → assert exact counts.

macro_rules! mem_test {
    ($name:ident, $body:block) => {
        #[test]
        fn $name() {
            runtime::MEM.reset();
            $body
        }
    };
}

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
    let (allocs, frees, _, _, _) = runtime::MEM.stats();    assert!(allocs >= 5, "should allocate >= 5 strings, got {}", allocs);
    assert_eq!(allocs, frees, "loop locals freed: {} allocs, {} frees", allocs, frees);
});

