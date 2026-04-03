//! Stdlib integration tests — compile Roca source with stdlib imports through the native JIT.
//! Proves the full pipeline: parse → check → stdlib resolution → JIT compile → execute.

use crate::test_runner;

fn run_roca(source: &str) -> test_runner::NativeTestResult {
    let file = roca_parse::parse(source);
    test_runner::run_tests(&file)
}

fn assert_passes(source: &str) {
    let result = run_roca(source);
    assert_eq!(result.failed, 0, "test failed:\n{}", result.output);
    assert!(result.passed >= 1, "no tests ran:\n{}", result.output);
}

// ─── Math ───────────────────────────────────────────

#[test]
fn stdlib_math_floor() {
    assert_passes(r#"
        import { Math } from std::math
        /// Floor
        pub fn floored(n: Number) -> Number {
            return Math.floor(n)
            test { self(3.7) == 3 }
        }
    "#);
}

#[test]
fn stdlib_math_abs() {
    assert_passes(r#"
        import { Math } from std::math
        /// Absolute value
        pub fn absolute(n: Number) -> Number {
            return Math.abs(n)
            test { self(-5) == 5 }
        }
    "#);
}

#[test]
fn stdlib_math_pow() {
    assert_passes(r#"
        import { Math } from std::math
        /// Power
        pub fn power(base: Number, exp: Number) -> Number {
            return Math.pow(base, exp)
            test { self(2, 10) == 1024 }
        }
    "#);
}

// ─── Path ───────────────────────────────────────────

#[test]
fn stdlib_path_join() {
    assert_passes(r#"
        import { Path } from std::path
        /// Join paths
        pub fn joined(a: String, b: String) -> String {
            return Path.join(a, b)
            test { self("/usr", "bin") == "/usr/bin" }
        }
    "#);
}

#[test]
fn stdlib_path_basename() {
    assert_passes(r#"
        import { Path } from std::path
        /// Get basename
        pub fn base(p: String) -> String {
            return Path.basename(p)
            test { self("/usr/bin/roca") == "roca" }
        }
    "#);
}

#[test]
fn stdlib_path_extension() {
    assert_passes(r#"
        import { Path } from std::path
        /// Get extension
        pub fn ext(p: String) -> String {
            return Path.extension(p)
            test { self("file.txt") == ".txt" }
        }
    "#);
}

// ─── Char ───────────────────────────────────────────

#[test]
fn stdlib_char_is_digit() {
    assert_passes(r#"
        import { Char } from std::char
        /// Check digit
        pub fn digit(c: String) -> Bool {
            return Char.isDigit(c)
            test { self("5") == true }
        }
    "#);
}

#[test]
fn stdlib_char_is_letter() {
    assert_passes(r#"
        import { Char } from std::char
        /// Check letter
        pub fn letter(c: String) -> Bool {
            return Char.isLetter(c)
            test { self("a") == true }
        }
    "#);
}

#[test]
fn stdlib_char_from_code() {
    assert_passes(r#"
        import { Char } from std::char
        /// From char code
        pub fn from_code(n: Number) -> String {
            return Char.fromCode(n)
            test { self(65) == "A" }
        }
    "#);
}

// ─── Crypto ─────────────────────────────────────────

#[test]
fn stdlib_crypto_sha256() {
    // SHA-256 of empty string is a known value
    let result = run_roca(r#"
        import { Crypto } from std::crypto
        /// Hash
        pub fn hash(s: String) -> String {
            return Crypto.sha256(s)
            test { self("hello") == "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824" }
        }
    "#);
    assert_eq!(result.failed, 0, "output: {}", result.output);
}

#[test]
fn stdlib_crypto_uuid() {
    // UUID is non-deterministic — just verify it compiles and runs
    let result = run_roca(r#"
        import { Crypto } from std::crypto
        /// Generate ID
        pub fn new_id() -> String {
            return Crypto.randomUUID()
            test {}
        }
    "#);
    assert_eq!(result.failed, 0, "output: {}", result.output);
}

// ─── Encoding ───────────────────────────────────────

#[test]
fn stdlib_encoding_btoa() {
    assert_passes(r#"
        import { Encoding } from std::encoding
        /// Encode
        pub fn encode(s: String) -> String {
            const result = Encoding.btoa(s)
            return result
            crash { Encoding.btoa -> fallback("") }
            test { self("hello") == "aGVsbG8=" }
        }
    "#);
}

#[test]
fn stdlib_encoding_atob() {
    assert_passes(r#"
        import { Encoding } from std::encoding
        /// Decode
        pub fn decode(s: String) -> String {
            const result = Encoding.atob(s)
            return result
            crash { Encoding.atob -> fallback("") }
            test { self("aGVsbG8=") == "hello" }
        }
    "#);
}

// ─── Time ───────────────────────────────────────────

#[test]
fn stdlib_time_now() {
    // Time.now() is non-deterministic — just verify it compiles
    let result = run_roca(r#"
        import { Time } from std::time
        /// Timestamp
        pub fn ts() -> Number {
            return Time.now()
            test {}
        }
    "#);
    assert_eq!(result.failed, 0, "output: {}", result.output);
}
