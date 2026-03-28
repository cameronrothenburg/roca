use super::harness::run;

#[test]
fn if_true_branch() {
    assert_eq!(run(
        r#"pub fn check(x: Number) -> String {
            if x > 0 { return "positive" }
            return "not positive"
            test { self(5) == "positive" }
        }"#,
        "console.log(check(5));",
    ), "positive");
}

#[test]
fn if_false_branch() {
    assert_eq!(run(
        r#"pub fn check(x: Number) -> String {
            if x > 0 { return "positive" }
            return "not positive"
            test { self(-1) == "not positive" }
        }"#,
        "console.log(check(-1));",
    ), "not positive");
}

#[test]
fn if_else() {
    assert_eq!(run(
        r#"pub fn sign(x: Number) -> String {
            if x > 0 {
                return "positive"
            } else {
                return "non-positive"
            }
            test {
                self(5) == "positive"
                self(-1) == "non-positive"
            }
        }"#,
        r#"console.log(sign(5)); console.log(sign(-1)); console.log(sign(0));"#,
    ), "positive\nnon-positive\nnon-positive");
}

#[test]
fn nested_if() {
    assert_eq!(run(
        r#"pub fn classify(x: Number) -> String {
            if x > 0 {
                if x > 100 { return "big" }
                return "small"
            }
            return "negative"
            test {
                self(200) == "big"
                self(5) == "small"
                self(-1) == "negative"
            }
        }"#,
        "console.log(classify(200)); console.log(classify(5)); console.log(classify(-1));",
    ), "big\nsmall\nnegative");
}

#[test]
fn clamp() {
    assert_eq!(run(
        r#"pub fn clamp(val: Number, min: Number, max: Number) -> Number {
            if val < min { return min }
            if val > max { return max }
            return val
            test {
                self(5, 0, 10) == 5
                self(-5, 0, 10) == 0
                self(50, 0, 10) == 10
            }
        }"#,
        "console.log(clamp(-5, 0, 10)); console.log(clamp(50, 0, 10)); console.log(clamp(5, 0, 10));",
    ), "0\n10\n5");
}

#[test]
fn for_loop() {
    assert_eq!(run(
        r#"pub fn sum_to(n: Number) -> Number {
            let total = 0
            let i = 1
            if i <= n {
                total = total + i
                i = i + 1
                if i <= n {
                    total = total + i
                    i = i + 1
                    if i <= n {
                        total = total + i
                    }
                }
            }
            return total
            test { self(3) == 6 }
        }"#,
        "console.log(sum_to(3));",
    ), "6");
}

#[test]
fn boolean_and() {
    assert_eq!(run(
        r#"pub fn both(a: Bool, b: Bool) -> Bool {
            if a {
                if b { return true }
            }
            return false
            test { self(true, true) == true self(true, false) == false }
        }"#,
        "console.log(both(true, true)); console.log(both(true, false)); console.log(both(false, true));",
    ), "true\nfalse\nfalse");
}
