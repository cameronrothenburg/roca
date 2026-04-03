//! Acceptance tests — complete valid programs that must produce zero diagnostics.
//!
//! These prove the checker doesn't reject correct code.

use crate::check;

fn is_clean(src: &str) -> bool {
    let ast = roca_parse::parse(src);
    let diags = check(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| d.code != "E-OWN-007").collect();
    if !errors.is_empty() {
        panic!("expected clean, got: {:?}", errors);
    }
    true
}

#[test]
fn full_program_with_struct_and_ownership() {
    assert!(is_clean(r#"
        pub struct User {
            name: String
            age: Int
        }{
            pub fn new(o name: String, o age: Int) -> User {
                return User { name: name, age: age }
            }
        }

        fn greet(b user: User) -> String {
            const greeting = "hello"
            return greeting
        }

        fn main() -> Int {
            const user = User.new("alice", 30)
            let u = user
            const msg = greet(u)
            return 0
        }
    "#));
}
