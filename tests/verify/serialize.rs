use super::harness::run;

#[test]
fn serializable_satisfies() {
    assert_eq!(run(
        r#"
        /// A user
        pub struct User {
            name: String
        }{}
        User satisfies Serializable {
            fn toJson() -> String {
                return self.name
                test { self() == "test" }
            }
        }
        "#,
        r#"console.log("ok");"#,
    ), "ok");
}

#[test]
fn deserializable_satisfies_generic() {
    assert_eq!(run(
        r#"
        /// A user
        pub struct User {
            name: String
            create(data: String) -> User, err {
                err invalid = "bad"
            }
        }{
            pub fn create(data: String) -> User, err {
                if data == "" { return err.invalid }
                return User { name: data }
                test {
                    self("cam") is Ok
                    self("") is err.invalid
                }
            }
        }
        User satisfies Deserializable<User> {
            fn parse(data: String) -> User, err {
                return User { name: data }
                test { self("cam") is Ok }
            }
        }
        "#,
        r#"
            const { value } = User.create("cam");
            console.log(value.name);
        "#,
    ), "cam");
}
