//! File I/O and constraint validation tests (core constraints)

use super::test_helpers::*;
use crate::runtime;

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
    let mut m = jit(r#"
        pub struct Config {
            port: Number { min: 1, max: 65535, default: 8080 }
        }{}
        pub fn make() -> Number {
            const c = Config { port: 8080 }
            return c.port
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    assert_eq!(f(), 8080.0);
}

#[test]
fn constraint_string_minlen_valid() {
    let mut m = jit(r#"
        pub struct User {
            name: String { minLen: 1, maxLen: 64, default: "anon" }
        }{}
        pub fn make() -> Number {
            const u = User { name: "cameron" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    assert_eq!(f(), 1.0);
}

#[test]
fn constraint_contains_valid() {
    let mut m = jit(r#"
        pub struct Email {
            value: String { contains: "@", default: "a@b.com" }
        }{}
        pub fn make() -> Number {
            const e = Email { value: "test@example.com" }
            return 1
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    f();
    assert!(runtime::constraint_violated(),
        "value without @ should violate contains: @");
}

// ─── Constraint: multiple constraints on same field ──

#[test]
fn constraint_number_min_and_max_valid() {
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    assert_eq!(f(), 50.0);
    assert!(!runtime::constraint_violated(), "50 is within [1, 100]");
}

#[test]
fn constraint_number_min_and_max_below() {
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    f();
    assert!(runtime::constraint_violated(), "0 violates min: 1");
}

#[test]
fn constraint_number_min_and_max_above() {
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    f();
    assert!(runtime::constraint_violated(), "200 violates max: 100");
}

#[test]
fn constraint_string_minlen_and_maxlen_valid() {
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    assert_eq!(f(), 1.0);
    assert!(!runtime::constraint_violated(), "'hello' (len 5) is within [3, 10]");
}

#[test]
fn constraint_string_minlen_violated() {
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    f();
    assert!(runtime::constraint_violated(), "'ab' (len 2) violates minLen: 3");
}

#[test]
fn constraint_string_maxlen_violated() {
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    f();
    assert!(runtime::constraint_violated(), "'this is way too long' violates maxLen: 10");
}

// ─── Constraint: boundary values ─────────────────────

#[test]
fn constraint_number_at_exact_min() {
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    assert_eq!(f(), 5.0);
    assert!(!runtime::constraint_violated(), "5 == min 5, should pass");
}

#[test]
fn constraint_number_at_exact_max() {
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    assert_eq!(f(), 100.0);
    assert!(!runtime::constraint_violated(), "100 == max 100, should pass");
}

#[test]
fn constraint_number_just_below_min() {
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    f();
    assert!(runtime::constraint_violated(), "4 < min 5, should fail");
}

#[test]
fn constraint_number_just_above_max() {
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
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make")) };
    f();
    assert!(runtime::constraint_violated(), "101 > max 100, should fail");
}
