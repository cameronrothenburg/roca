use super::harness::run;

// ─── Valid comparisons — JS execution ───────────────────

#[test]
fn string_equality() {
    assert_eq!(run(
        r#"pub fn check(a: String, b: String) -> Bool {
            return a == b
            test { self("hello", "hello") == true self("a", "b") == false }
        }"#,
        r#"console.log(check("same", "same")); console.log(check("a", "b"));"#,
    ), "true\nfalse");
}

#[test]
fn string_inequality() {
    assert_eq!(run(
        r#"pub fn check(a: String, b: String) -> Bool {
            return a != b
            test { self("a", "b") == true self("a", "a") == false }
        }"#,
        r#"console.log(check("x", "y")); console.log(check("x", "x"));"#,
    ), "true\nfalse");
}

#[test]
fn string_ordering() {
    assert_eq!(run(
        r#"pub fn comes_first(a: String, b: String) -> Bool {
            return a < b
            test { self("a", "b") == true self("z", "a") == false }
        }"#,
        r#"console.log(comes_first("apple", "banana")); console.log(comes_first("z", "a"));"#,
    ), "true\nfalse");
}

#[test]
fn number_equality() {
    assert_eq!(run(
        r#"pub fn eq(a: Number, b: Number) -> Bool {
            return a == b
            test { self(1, 1) == true self(1, 2) == false }
        }"#,
        "console.log(eq(42, 42)); console.log(eq(1, 2));",
    ), "true\nfalse");
}

#[test]
fn number_ordering() {
    assert_eq!(run(
        r#"pub fn greater(a: Number, b: Number) -> Bool {
            return a > b
            test { self(10, 5) == true self(1, 10) == false }
        }"#,
        "console.log(greater(10, 5)); console.log(greater(1, 100));",
    ), "true\nfalse");
}

#[test]
fn number_lte_gte() {
    assert_eq!(run(
        r#"pub fn in_range(val: Number, min: Number, max: Number) -> Bool {
            if val >= min {
                if val <= max { return true }
            }
            return false
            test { self(5, 0, 10) == true self(15, 0, 10) == false }
        }"#,
        "console.log(in_range(5, 0, 10)); console.log(in_range(-1, 0, 10)); console.log(in_range(10, 0, 10));",
    ), "true\nfalse\ntrue");
}

#[test]
fn bool_equality() {
    assert_eq!(run(
        r#"pub fn same(a: Bool, b: Bool) -> Bool {
            return a == b
            test { self(true, true) == true self(true, false) == false }
        }"#,
        "console.log(same(true, true)); console.log(same(true, false));",
    ), "true\nfalse");
}

// ─── Checker catches bad comparisons ────────────────────

#[test]
fn cross_type_comparison_caught() {
    let file = roca::parse::parse(r#"
        pub fn bad(name: String, age: Number) -> Bool {
            return name == age
            test { self("cam", 25) == false }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "type-mismatch"),
        "should catch String == Number, got: {:?}", errors);
}

#[test]
fn struct_comparison_caught() {
    let file = roca::parse::parse(r#"
        pub struct Email { value: String }{}
        pub fn bad(a: Email, b: Email) -> Bool {
            return a == b
            test { self(a, b) == false }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "struct-comparison"),
        "should catch struct == struct, got: {:?}", errors);
}

#[test]
fn bool_ordering_caught() {
    let file = roca::parse::parse(r#"
        pub fn bad(a: Bool, b: Bool) -> Bool {
            return a > b
            test { self(true, false) == true }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "invalid-ordering"),
        "should catch Bool ordering, got: {:?}", errors);
}

#[test]
fn field_comparison_works() {
    // Comparing struct fields (which are primitives) should pass
    let file = roca::parse::parse(r#"
        pub struct Email { value: String }{}
        pub fn same_email(a: Email, b: Email) -> Bool {
            return a.value == b.value
            test { self(a, b) == true }
        }
    "#);
    let errors = roca::check::check(&file);
    let comp_errors: Vec<_> = errors.iter()
        .filter(|e| e.code == "type-mismatch" || e.code == "struct-comparison")
        .collect();
    assert!(comp_errors.is_empty(),
        "field comparison should pass, got: {:?}", comp_errors);
}

#[test]
fn string_number_mismatch_in_if() {
    let file = roca::parse::parse(r#"
        pub fn bad(name: String) -> Bool {
            if name == 42 { return true }
            return false
            test { self("cam") == false }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "type-mismatch"),
        "should catch String == Number in if condition, got: {:?}", errors);
}

#[test]
fn inferred_type_comparison() {
    // Variable types inferred from assignment — comparison should be checked
    let file = roca::parse::parse(r#"
        pub fn bad() -> Bool {
            const name = "hello"
            const age = 25
            return name == age
            test { self() == false }
        }
    "#);
    let errors = roca::check::check(&file);
    assert!(errors.iter().any(|e| e.code == "type-mismatch"),
        "should catch inferred String == Number, got: {:?}", errors);
}

#[test]
fn same_type_comparison_passes() {
    let file = roca::parse::parse(r#"
        pub fn check(a: String, b: String) -> Bool {
            return a == b
            test { self("a", "a") == true }
        }
    "#);
    let errors = roca::check::check(&file);
    let comp_errors: Vec<_> = errors.iter()
        .filter(|e| e.code == "type-mismatch" || e.code == "struct-comparison" || e.code == "invalid-ordering")
        .collect();
    assert!(comp_errors.is_empty(), "same-type comparison should pass, got: {:?}", comp_errors);
}

#[test]
fn string_gte_works() {
    // String ordering is valid
    assert_eq!(run(
        r#"pub fn gte(a: String, b: String) -> Bool {
            return a >= b
            test { self("b", "a") == true self("a", "b") == false }
        }"#,
        r#"console.log(gte("b", "a")); console.log(gte("a", "z"));"#,
    ), "true\nfalse");
}
