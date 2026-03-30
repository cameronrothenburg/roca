use super::harness::run;

#[test]
fn self_field_assign() {
    assert_eq!(run(
        r#"pub struct Counter {
            count: Number
            increment() -> Counter
        }{
            fn increment() -> Counter {
                self.count = self.count + 1
                return self
                test {}
            }
        }

        pub fn test_counter() -> Number {
            let c = Counter { count: 0 }
            const c2 = c.increment()
            return c2.count
            crash { c.increment -> skip }
            test { self() == 1 }
        }"#,
        r#"
            console.log(test_counter());
        "#,
    ), "1");
}
