use super::harness::run;

#[test]
fn crypto_random_uuid() {
    let result = run(
        r#"
        import { Crypto } from std::crypto
        /// Gets a UUID
        pub fn new_id() -> String {
            return Crypto.randomUUID()
            test {}
        }
        "#,
        r#"
            const id = new_id();
            console.log(id.length === 36 ? "ok" : "fail:" + id.length);
        "#,
    );
    assert_eq!(result, "ok");
}

#[test]
fn crypto_uuid_unique() {
    let result = run(
        r#"
        import { Crypto } from std::crypto
        /// Gets two UUIDs
        pub fn two_ids() -> String {
            const a = Crypto.randomUUID()
            const b = Crypto.randomUUID()
            if a == b { return "same" }
            return "different"
            test {}
        }
        "#,
        r#"console.log(two_ids());"#,
    );
    assert_eq!(result, "different");
}

#[test]
fn crypto_sha256() {
    // SHA-256 of empty string is a known value
    let result = run(
        r#"
        import { Crypto } from std::crypto
        /// Hashes data
        pub fn hash(s: String) -> String {
            const h = wait Crypto.sha256(s)
            return h
            crash { Crypto.sha256 -> skip }
            test { self("test") == self("test") }
        }
        "#,
        r#"
            const h = await hash("");
            console.log(h.length);
        "#,
    );
    assert_eq!(result, "64", "SHA-256 hex should be 64 chars");
}
