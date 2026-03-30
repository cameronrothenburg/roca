use super::harness::run;

#[test]
fn satisfies_string_registry_check() {
    // Email satisfies String — compiler should track this
    let file = roca::parse::parse(r#"
        pub struct Email { value: String }{}
        Email satisfies String {
            fn trim() -> String { return self.value test {} }
            fn toString() -> String { return self.value test {} }
        }
    "#);
    let reg = roca::check::registry::ContractRegistry::build(&file);
    assert!(reg.type_satisfies("Email", "String"));
    assert!(reg.type_accepts("String", "Email"));
}

#[test]
fn does_not_satisfy_unimplemented() {
    let file = roca::parse::parse(r#"
        pub struct Email { value: String }{}
    "#);
    let reg = roca::check::registry::ContractRegistry::build(&file);
    assert!(!reg.type_satisfies("Email", "String"));
    assert!(!reg.type_accepts("String", "Email"));
}

#[test]
fn multiple_satisfies_tracked() {
    let file = roca::parse::parse(r#"
        contract Loggable { to_log() -> String }
        pub struct Email { value: String }{}
        Email satisfies String {
            fn trim() -> String { return self.value test {} }
        }
        Email satisfies Loggable {
            fn to_log() -> String { return self.value test {} }
        }
    "#);
    let reg = roca::check::registry::ContractRegistry::build(&file);
    assert!(reg.type_satisfies("Email", "String"));
    assert!(reg.type_satisfies("Email", "Loggable"));
    assert!(!reg.type_satisfies("Email", "Number"));
}

#[test]
fn secret_satisfies_loggable_but_not_string() {
    let file = roca::parse::parse(r#"
        contract Loggable { to_log() -> String }
        pub struct Secret { value: String }{}
        Secret satisfies Loggable {
            fn to_log() -> String { return "REDACTED" test {} }
        }
    "#);
    let reg = roca::check::registry::ContractRegistry::build(&file);
    assert!(reg.type_satisfies("Secret", "Loggable"));
    assert!(!reg.type_satisfies("Secret", "String"));
    // Secret can be logged but NOT passed where String is expected
    assert!(reg.type_accepts("Loggable", "Secret"));
    assert!(!reg.type_accepts("String", "Secret"));
}

#[test]
fn email_satisfies_string_can_be_used_as_string() {
    // Email with a custom contract that has trim, and passed to a function expecting it
    assert_eq!(run(
        r#"
        contract Trimmable { trim() -> String }

        pub struct Email {
            value: String
            create(raw: String) -> Email
        }{
            pub fn create(raw: String) -> Email {
                return Email { value: raw }
                test {}
            }
        }

        Email satisfies Trimmable {
            fn trim() -> String { return self.value.trim() crash { self.value.trim -> skip } test {} }
        }

        pub fn show_trimmed(s: String) -> String {
            return s.trim()
            crash { s.trim -> skip }
            test { self(" hello ") == "hello" }
        }
        "#,
        r#"
            const email = Email.create(" cam@test.com ");
            // Call trim directly on email — it satisfies Trimmable
            console.log(email.trim());
        "#,
    ), "cam@test.com");
}
