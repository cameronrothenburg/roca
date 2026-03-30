use super::harness::run;

// ─── btoa / atob ─────────────────────────────────────────

#[test]
fn btoa_encodes_to_base64() {
    assert_eq!(run(
        r#"
        import { Encoding } from std::encoding
        pub fn to_b64(s: String) -> String {
            const result = Encoding.btoa(s)
            return result
            crash { Encoding.btoa -> halt }
            test { self("hello") == "aGVsbG8=" }
        }
        "#,
        r#"
            console.log(to_b64("hello"));
        "#,
    ), "aGVsbG8=");
}

#[test]
fn atob_decodes_from_base64() {
    assert_eq!(run(
        r#"
        import { Encoding } from std::encoding
        pub fn from_b64(s: String) -> String {
            const result = Encoding.atob(s)
            return result
            crash { Encoding.atob -> halt }
            test { self("aGVsbG8=") == "hello" }
        }
        "#,
        r#"
            console.log(from_b64("aGVsbG8="));
        "#,
    ), "hello");
}

#[test]
fn btoa_atob_roundtrip() {
    assert_eq!(run(
        r#"
        import { Encoding } from std::encoding
        pub fn roundtrip(s: String) -> String {
            const encoded = Encoding.btoa(s)
            const decoded = Encoding.atob(encoded)
            return decoded
            crash {
                Encoding.btoa -> halt
                Encoding.atob -> halt
            }
            test { self("test data") == "test data" }
        }
        "#,
        r#"
            console.log(roundtrip("test data"));
        "#,
    ), "test data");
}
