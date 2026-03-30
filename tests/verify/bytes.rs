use super::harness::run;

// ─── Bytes — JS execution ───────────────────────────────

#[test]
fn bytes_from_string() {
    assert_eq!(run(
        r#"pub fn encode(s: String) -> Number {
            const encoder = TextEncoder()
            const bytes = encoder.encode(s)
            return bytes.byteLength
            crash {
                TextEncoder -> skip
                encoder.encode -> skip
                bytes.byteLength -> skip
            }
            test { self("hello") == 5 }
        }"#,
        r#"console.log(encode("hello"));"#,
    ), "5");
}

#[test]
fn bytes_to_string() {
    assert_eq!(run(
        r#"pub fn decode() -> String {
            const encoder = TextEncoder()
            const bytes = encoder.encode("hello")
            const decoder = TextDecoder()
            const result = decoder.decode(bytes)
            return result
            crash {
                TextEncoder -> skip
                encoder.encode -> skip
                TextDecoder -> skip
                decoder.decode -> skip
            }
            test { self() == "hello" }
        }"#,
        "console.log(decode());",
    ), "hello");
}

#[test]
fn uint8array_operations() {
    assert_eq!(run(
        r#"pub fn byte_work() -> Number {
            const arr = Uint8Array(4)
            return arr.byteLength
            crash {
                Uint8Array -> skip
                arr.byteLength -> skip
            }
            test { self() == 4 }
        }"#,
        "console.log(byte_work());",
    ), "4");
}

// ─── Buffer pattern (ArrayBuffer) ───────────────────────

#[test]
fn buffer_create() {
    assert_eq!(run(
        r#"pub fn make_buffer() -> Number {
            const buf = ArrayBuffer(16)
            return buf.byteLength
            crash {
                ArrayBuffer -> skip
                buf.byteLength -> skip
            }
            test { self() == 16 }
        }"#,
        "console.log(make_buffer());",
    ), "16");
}

// ─── Base64 encode/decode ───────────────────────────────

#[test]
fn base64_roundtrip() {
    assert_eq!(run(
        r#"pub fn roundtrip(s: String) -> String {
            const encoded = btoa(s)
            const decoded = atob(encoded)
            return decoded
            crash {
                btoa -> skip
                atob -> skip
            }
            test { self("hello") == "hello" }
        }"#,
        r#"console.log(roundtrip("hello world"));"#,
    ), "hello world");
}

// ─── Bytes contract checking ────────────────────────────

#[test]
fn bytes_has_methods() {
    let file = roca::parse::parse("");
    let reg = roca::check::registry::ContractRegistry::build(&file);
    assert!(reg.has_method("Bytes", "toString"));
    assert!(reg.has_method("Bytes", "slice"));
    assert!(reg.has_method("Bytes", "at"));
    assert!(reg.has_method("Bytes", "toHex"));
    assert!(reg.has_method("Bytes", "toBase64"));
    assert!(reg.has_method("Bytes", "toArray"));
    assert!(reg.has_method("Bytes", "byteLength"));
    assert!(reg.has_method("Bytes", "to_log"));
}

#[test]
fn buffer_has_methods() {
    let file = roca::parse::parse("");
    let reg = roca::check::registry::ContractRegistry::build(&file);
    assert!(reg.has_method("Buffer", "write"));
    assert!(reg.has_method("Buffer", "writeString"));
    assert!(reg.has_method("Buffer", "writeByte"));
    assert!(reg.has_method("Buffer", "toBytes"));
    assert!(reg.has_method("Buffer", "toString"));
    assert!(reg.has_method("Buffer", "byteLength"));
    assert!(reg.has_method("Buffer", "clear"));
}
