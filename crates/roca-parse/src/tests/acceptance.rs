//! Acceptance tests — complete valid programs that must produce zero diagnostics.
//!
//! These prove the checker doesn't reject correct code.

fn is_clean(src: &str) -> bool {
    let result = crate::parse(src);
    if !result.errors.is_empty() {
        panic!("expected clean, got: {:?}", result.errors);
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
