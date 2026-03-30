use super::harness::run;

#[test]
fn satisfies_adds_instance_method() {
    assert_eq!(run(
        r#"contract Stringable { to_string() -> String }

        pub struct Email {
            value: String
        }{}

        Email satisfies Stringable {
            fn to_string() -> String {
                return self.value
                test { self() == "test" }
            }
        }"#,
        r#"
            const e = new Email({ value: "cam@test.com" });
            console.log(e.to_string());
        "#,
    ), "cam@test.com");
}

#[test]
fn satisfies_two_contracts() {
    assert_eq!(run(
        r#"contract Stringable { to_string() -> String }
        contract Measurable { len() -> Number }

        pub struct Name {
            value: String
        }{}

        Name satisfies Stringable {
            fn to_string() -> String {
                return self.value
                test { self() == "test" }
            }
        }

        Name satisfies Measurable {
            fn len() -> Number {
                return self.value.length
                test { self() == 4 }
            }
        }"#,
        r#"
            const n = new Name({ value: "Cameron" });
            console.log(n.to_string());
            console.log(n.len());
        "#,
    ), "Cameron\n7");
}

#[test]
fn satisfies_with_struct_own_methods() {
    assert_eq!(run(
        r#"contract Stringable { to_string() -> String }

        pub struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err invalid = "bad"
            }
        }{
            fn validate(raw: String) -> Email, err {
                if raw == "" { return err.invalid }
                return Email { value: raw }
                test { self("a@b.com") is Ok self("") is err.invalid }
            }
        }

        Email satisfies Stringable {
            fn to_string() -> String {
                return self.value
                test { self() == "test" }
            }
        }"#,
        r#"
            const { value: e, err: _ } = Email.validate("cam@test.com");
            console.log(e.to_string());
            console.log(e.value);
        "#,
    ), "cam@test.com\ncam@test.com");
}

#[test]
fn satisfies_method_uses_self_field() {
    assert_eq!(run(
        r#"contract Describable { describe() -> String }

        pub struct Product {
            name: String
            price: Number
        }{}

        Product satisfies Describable {
            fn describe() -> String {
                return self.name + " costs " + self.price
                test { self() == "Widget costs 10" }
            }
        }"#,
        r#"
            const p = new Product({ name: "Widget", price: 10 });
            console.log(p.describe());
        "#,
    ), "Widget costs 10");
}
