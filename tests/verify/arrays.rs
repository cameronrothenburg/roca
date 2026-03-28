use super::harness::run;

#[test]
fn array_literal() {
    assert_eq!(run(
        r#"pub fn nums() -> Number {
            const arr = [1, 2, 3]
            return arr[0]
            test { self() == 1 }
        }"#,
        "console.log(nums());",
    ), "1");
}

#[test]
fn array_index_access() {
    assert_eq!(run(
        r#"pub fn second(items: String) -> Number {
            const arr = [10, 20, 30]
            return arr[1]
            test { self("x") == 20 }
        }"#,
        r#"console.log(second("x"));"#,
    ), "20");
}

#[test]
fn array_length() {
    assert_eq!(run(
        r#"pub fn count() -> Number {
            const arr = [1, 2, 3, 4, 5]
            return arr.length
            test { self() == 5 }
        }"#,
        "console.log(count());",
    ), "5");
}

#[test]
fn empty_array() {
    assert_eq!(run(
        r#"pub fn empty() -> Number {
            const arr = []
            return arr.length
            test { self() == 0 }
        }"#,
        "console.log(empty());",
    ), "0");
}

#[test]
fn array_of_strings() {
    assert_eq!(run(
        r#"pub fn first_name() -> String {
            const names = ["alice", "bob", "cam"]
            return names[0]
            test { self() == "alice" }
        }"#,
        "console.log(first_name());",
    ), "alice");
}

#[test]
fn for_in_array() {
    assert_eq!(run(
        r#"pub fn sum() -> Number {
            const nums = [1, 2, 3]
            let total = 0
            for n in nums {
                total = total + n
            }
            return total
            test { self() == 6 }
        }"#,
        "console.log(sum());",
    ), "6");
}

#[test]
fn array_method_push() {
    assert_eq!(run(
        r#"pub fn build() -> Number {
            let arr = [1, 2]
            arr.push(3)
            return arr.length
            crash { arr.push -> halt }
            test { self() == 3 }
        }"#,
        "console.log(build());",
    ), "3");
}

#[test]
fn nested_array_access() {
    assert_eq!(run(
        r#"pub fn get() -> Number {
            const matrix = [[1, 2], [3, 4]]
            return matrix[1][0]
            test { self() == 3 }
        }"#,
        "console.log(get());",
    ), "3");
}
