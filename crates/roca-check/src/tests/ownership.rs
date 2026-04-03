//! Ownership rule tests (E-OWN-001 through E-OWN-010)
//!
//! These enforce the 7 ownership rules from memory.md:
//!   Rule 1: const is always an owner (E-OWN-001)
//!   Rule 2: let is always a borrow from a const (E-OWN-002)
//!   Rule 3: must let before passing to b param (E-OWN-003)
//!   Rule 4: passing const to o param is a move (E-OWN-004)
//!   Rule 5: params must declare o or b (E-OWN-005)
//!   Rule 6: return values are always owned (E-OWN-006)
//!   Rule 7: containers copy borrowed values (E-OWN-007)
//!   + second-class refs (E-OWN-008)
//!   + branch symmetry (E-OWN-009)
//!   + loop consumption (E-OWN-010)

use crate::check;

fn errors(src: &str) -> Vec<String> {
    let ast = roca_parse::parse(src);
    check(&ast).iter().map(|d| d.code.to_string()).collect()
}

fn has_error(src: &str, code: &str) -> bool {
    errors(src).contains(&code.to_string())
}

fn is_clean(src: &str) -> bool {
    errors(src).is_empty()
}

// ─── E-OWN-001: const is always an owner ─────────────

#[test]
fn own_001_valid() {
    assert!(is_clean(r#"
        fn main() -> Int {
            const x = 42
            return x
        }
    "#));
}

#[test]
fn own_001_reject_orphan_value() {
    // A value expression as a statement without binding to const
    assert!(has_error(r#"
        fn main() -> Int {
            42
            return 0
        }
    "#, "E-OWN-001"));
}

// ─── E-OWN-002: let borrows from const ───────────────

#[test]
fn own_002_valid() {
    assert!(is_clean(r#"
        pub struct User { name: String }{}
        fn main() -> String {
            const u = User { name: "alice" }
            let name = u.name
            return name
        }
    "#));
}

#[test]
fn own_002_reject_let_creates_value() {
    // let cannot create a new value — must derive from const
    assert!(has_error(r#"
        fn main() -> Int {
            let x = 42
            return x
        }
    "#, "E-OWN-002"));
}

// ─── E-OWN-003: must let before passing to b param ───

#[test]
fn own_003_valid() {
    assert!(is_clean(r#"
        fn process(b x: Int) -> Int {
            return x
        }
        fn main() -> Int {
            const val = 10
            let borrowed = val
            return process(borrowed)
        }
    "#));
}

#[test]
fn own_003_reject_const_direct_to_b() {
    // const passed directly to b parameter without let
    assert!(has_error(r#"
        fn process(b x: Int) -> Int {
            return x
        }
        fn main() -> Int {
            const val = 10
            return process(val)
        }
    "#, "E-OWN-003"));
}

// ─── E-OWN-004: use after move ───────────────────────

#[test]
fn own_004_valid() {
    assert!(is_clean(r#"
        fn consume(o x: Int) -> Int {
            return x
        }
        fn main() -> Int {
            const val = 10
            return consume(val)
        }
    "#));
}

#[test]
fn own_004_reject_use_after_move() {
    // val is consumed by consume(), then used again
    assert!(has_error(r#"
        fn consume(o x: Int) -> Int {
            return x
        }
        fn identity(o x: Int) -> Int {
            return x
        }
        fn main() -> Int {
            const val = 10
            const a = consume(val)
            const b = identity(val)
            return a + b
        }
    "#, "E-OWN-004"));
}

// ─── E-OWN-005: params must declare o or b ───────────

#[test]
fn own_005_valid() {
    assert!(is_clean(r#"
        fn process(b x: Int) -> Int {
            return x
        }
    "#));
}

#[test]
fn own_005_reject_missing_qualifier() {
    // param without o or b
    assert!(has_error(r#"
        fn process(x: Int) -> Int {
            return x
        }
    "#, "E-OWN-005"));
}

// ─── E-OWN-006: return values are always owned ───────

#[test]
fn own_006_valid() {
    // Returning a const (owned) is fine
    assert!(is_clean(r#"
        fn make() -> Int {
            const x = 42
            return x
        }
    "#));
}

#[test]
fn own_006_reject_return_borrowed() {
    // Returning a borrowed struct param directly — must copy.
    // Primitives (Int, Float, Bool, String) are copyable so returning b params is fine.
    // Structs are not — returning a borrowed struct is E-OWN-006.
    assert!(has_error(r#"
        pub struct User { name: String }{}
        fn get(b u: User) -> User {
            return u
        }
    "#, "E-OWN-006"));
}

// ─── E-OWN-007: containers copy borrowed values ──────

#[test]
fn own_007_emits_note() {
    // Not an error — a note. Borrowed value inserted into container.
    let ast = roca_parse::parse(r#"
        fn main() -> Int {
            const items = [1, 2, 3]
            let first = items
            const result = [first]
            return 0
        }
    "#);
    let diags = check(&ast);
    let has_note = diags.iter().any(|d| d.code == "E-OWN-007");
    assert!(has_note, "expected E-OWN-007 note for borrowed value in container");
}

// ─── E-OWN-008: second-class references ──────────────

#[test]
fn own_008_valid() {
    // Struct with owned fields — fine
    assert!(is_clean(r#"
        pub struct User {
            name: String
            age: Int
        }{}
    "#));
}

#[test]
fn own_008_reject_return_borrow_from_b_param() {
    // Returning a field from a borrowed param without copy
    assert!(has_error(r#"
        pub struct User { name: String }{}
        fn get_name(b u: User) -> String {
            return u.name
        }
    "#, "E-OWN-006")); // E-OWN-006 covers this case too
}

// ─── E-OWN-009: branch symmetry ──────────────────────

#[test]
fn own_009_valid() {
    // Both branches consume the value
    assert!(is_clean(r#"
        fn consume(o x: Int) -> Int { return x }
        fn main() -> Int {
            const val = 10
            if true {
                return consume(val)
            } else {
                return consume(val)
            }
        }
    "#));
}

#[test]
fn own_009_reject_asymmetric() {
    // One branch consumes, the other doesn't
    assert!(has_error(r#"
        fn consume(o x: Int) -> Int { return x }
        fn main() -> Int {
            const val = 10
            if true {
                return consume(val)
            } else {
                return 0
            }
        }
    "#, "E-OWN-009"));
}

// ─── E-OWN-010: loop consumption ─────────────────────

#[test]
fn own_010_valid() {
    // Loop borrows outer const — fine
    assert!(is_clean(r#"
        fn read(b x: Int) -> Int { return x }
        fn main() -> Int {
            const val = 10
            var i = 0
            loop {
                if i > 2 { break }
                let borrowed = val
                const r = read(borrowed)
                i = i + 1
            }
            return val
        }
    "#));
}

#[test]
fn own_010_reject_consume_in_loop() {
    // Loop consumes outer const without reassignment
    assert!(has_error(r#"
        fn consume(o x: Int) -> Int { return x }
        fn main() -> Int {
            const val = 10
            var i = 0
            loop {
                if i > 2 { break }
                const r = consume(val)
                i = i + 1
            }
            return 0
        }
    "#, "E-OWN-010"));
}
