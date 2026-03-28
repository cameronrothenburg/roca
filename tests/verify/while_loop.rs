use super::harness::run;

#[test]
fn basic_while() {
    assert_eq!(run(
        r#"pub fn count_to(n: Number) -> Number {
            let i = 0
            let total = 0
            while i < n {
                total = total + i
                i = i + 1
            }
            return total
            test { self(5) == 10 }
        }"#,
        "console.log(count_to(5)); console.log(count_to(0));",
    ), "10\n0");
}

#[test]
fn while_with_break() {
    assert_eq!(run(
        r#"pub fn find_first_gt(threshold: Number) -> Number {
            let i = 0
            while i < 100 {
                if i > threshold { break }
                i = i + 1
            }
            return i
            test { self(5) == 6 }
        }"#,
        "console.log(find_first_gt(5)); console.log(find_first_gt(0));",
    ), "6\n1");
}

#[test]
fn while_with_continue() {
    assert_eq!(run(
        r#"pub fn sum_odd(n: Number) -> Number {
            let i = 0
            let total = 0
            while i < n {
                i = i + 1
                if i == 2 { continue }
                if i == 4 { continue }
                total = total + i
            }
            return total
            test { self(5) == 9 }
        }"#,
        "console.log(sum_odd(5));",
    ), "9");
}

#[test]
fn while_string_builder() {
    assert_eq!(run(
        r#"pub fn repeat_str(s: String, n: Number) -> String {
            let result = ""
            let i = 0
            while i < n {
                result = result + s
                i = i + 1
            }
            return result
            test { self("ab", 3) == "ababab" }
        }"#,
        r#"console.log(repeat_str("ha", 3));"#,
    ), "hahaha");
}

#[test]
fn nested_while() {
    assert_eq!(run(
        r#"pub fn grid(rows: Number, cols: Number) -> Number {
            let count = 0
            let r = 0
            while r < rows {
                let c = 0
                while c < cols {
                    count = count + 1
                    c = c + 1
                }
                r = r + 1
            }
            return count
            test { self(3, 4) == 12 }
        }"#,
        "console.log(grid(3, 4));",
    ), "12");
}

#[test]
fn while_false_never_runs() {
    assert_eq!(run(
        r#"pub fn never() -> Number {
            let x = 0
            while false {
                x = 999
            }
            return x
            test { self() == 0 }
        }"#,
        "console.log(never());",
    ), "0");
}
