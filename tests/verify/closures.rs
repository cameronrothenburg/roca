use super::harness::run;

#[test]
fn map_with_closure() {
    assert_eq!(run(
        r#"pub fn double_all() -> String {
            const nums = [1, 2, 3]
            const doubled = nums.map(fn(x) -> x * 2)
            return doubled.join(",")
            crash {
                nums.map -> halt
                doubled.join -> halt
            }
            test { self() == "2,4,6" }
        }"#,
        "console.log(double_all());",
    ), "2,4,6");
}

#[test]
fn filter_with_closure() {
    assert_eq!(run(
        r#"pub fn only_big() -> String {
            const nums = [1, 5, 10, 15, 3]
            const big = nums.filter(fn(x) -> x > 5)
            return big.join(",")
            crash {
                nums.filter -> halt
                big.join -> halt
            }
            test { self() == "10,15" }
        }"#,
        "console.log(only_big());",
    ), "10,15");
}

#[test]
fn closure_string_transform() {
    assert_eq!(run(
        r#"pub fn shout() -> String {
            const words = ["hello", "world"]
            const upper = words.map(fn(w) -> w.toUpperCase())
            return upper.join(" ")
            crash {
                words.map -> halt
                upper.join -> halt
            }
            test { self() == "HELLO WORLD" }
        }"#,
        "console.log(shout());",
    ), "HELLO WORLD");
}

#[test]
fn closure_no_params() {
    assert_eq!(run(
        r#"pub fn make_array() -> String {
            const arr = [1, 2, 3]
            const result = arr.map(fn(x) -> "item")
            return result.join(",")
            crash {
                arr.map -> halt
                result.join -> halt
            }
            test { self() == "item,item,item" }
        }"#,
        "console.log(make_array());",
    ), "item,item,item");
}

#[test]
fn closure_multi_param() {
    assert_eq!(run(
        r#"pub fn with_index() -> String {
            const arr = ["a", "b", "c"]
            const result = arr.map(fn(item, i) -> String(i) + ":" + item)
            return result.join(",")
            crash {
                arr.map -> halt
                result.join -> halt
                String -> halt
            }
            test { self() == "0:a,1:b,2:c" }
        }"#,
        "console.log(with_index());",
    ), "0:a,1:b,2:c");
}

#[test]
fn closure_in_variable() {
    assert_eq!(run(
        r#"pub fn apply() -> Number {
            const double = fn(x) -> x * 2
            return double(5)
            crash { double -> halt }
            test { self() == 10 }
        }"#,
        "console.log(apply());",
    ), "10");
}

#[test]
fn chained_map_filter() {
    assert_eq!(run(
        r#"pub fn process() -> String {
            const nums = [1, 2, 3, 4, 5, 6]
            const result = nums.filter(fn(x) -> x > 2).map(fn(x) -> x * 10)
            return result.join(",")
            crash {
                nums.filter -> halt
                result.join -> halt
            }
            test { self() == "30,40,50,60" }
        }"#,
        "console.log(process());",
    ), "30,40,50,60");
}
