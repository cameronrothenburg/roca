# Stdlib Modules

Standard library modules are imported with `std::` syntax. The JS runtime wrappers are inlined automatically — no separate runtime file is emitted. Compiled output uses standard Web APIs (`globalThis.fetch`, `globalThis.URL`, etc.) and works in browsers, Node, Bun, and Cloudflare Workers.

## Import syntax

```roca
import { JSON } from std::json
import { Http } from std::http
import { Url } from std::url
import { Crypto } from std::crypto
import { Encoding } from std::encoding
import { Time } from std::time
```

## std::json

Parse and stringify JSON. The `JSON` type has typed accessor methods.

```roca
import { JSON } from std::json

pub fn parse_config(raw: String) -> String {
    const data = JSON.parse(raw)
    return data.getString("name")
    crash { JSON.parse -> fallback(fn(e) -> JSON) }
    test { self('{"name":"cam"}') == "cam" }
}
```

| Method | Signature |
|--------|-----------|
| `parse(text)` | `-> JSON, err { err parse_failed }` |
| `stringify(value)` | `-> String` |
| `get(key)` | `-> Optional<JSON>` |
| `getString(key)` | `-> Optional<String>` |
| `getNumber(key)` | `-> Optional<Number>` |
| `getBool(key)` | `-> Optional<Bool>` |
| `getArray(key)` | `-> Optional<Array<JSON>>` |
| `toString()` | `-> String` |

## std::http

HTTP requests. Uses `globalThis.fetch` under the hood.

```roca
import { Http } from std::http

pub fn get_status(url: String) -> Number {
    const resp = wait Http.get(url)
    return resp.status()
    crash { Http.get -> fallback(fn(e) -> Http) }
    test { self("https://example.com") == 200 }
}
```

| Method | Signature |
|--------|-----------|
| `get(url)` | `-> Http, err { err network, err abort }` |
| `post(url, body)` | `-> Http, err { err network, err abort }` |
| `put(url, body)` | `-> Http, err { err network, err abort }` |
| `patch(url, body)` | `-> Http, err { err network, err abort }` |
| `delete(url)` | `-> Http, err { err network, err abort }` |
| `status()` | `-> Number` |
| `ok()` | `-> Bool` |
| `text()` | `-> String, err { err consumed }` |
| `json()` | `-> JSON, err { err consumed, err parse }` |
| `header(name)` | `-> Optional<String>` |

## std::url

URL parsing backed by the WHATWG URL standard.

```roca
import { Url } from std::url

pub fn get_host(raw: String) -> String {
    const url = Url.parse(raw)
    return url.hostname()
    crash { Url.parse -> fallback(fn(e) -> "") }
    test { self("https://example.com/path") == "example.com" }
}
```

| Method | Signature |
|--------|-----------|
| `parse(raw)` | `-> Url, err { err parse_failed }` |
| `isValid(raw)` | `-> Bool` |
| `href()` | `-> String` |
| `origin()` | `-> String` |
| `protocol()` | `-> String` |
| `hostname()` | `-> String` |
| `host()` | `-> String` |
| `port()` | `-> String` |
| `pathname()` | `-> String` |
| `search()` | `-> String` |
| `hash()` | `-> String` |
| `getParam(name)` | `-> Optional<String>` |
| `hasParam(name)` | `-> Bool` |

## std::crypto

Cryptographic operations. UUID generation and hashing.

```roca
import { Crypto } from std::crypto

pub fn new_id() -> String {
    return Crypto.randomUUID()
    test {}
}
```

| Method | Signature |
|--------|-----------|
| `randomUUID()` | `-> String` |
| `sha256(data)` | `-> String` |
| `sha512(data)` | `-> String` |

## std::encoding

Text encoding/decoding and base64.

```roca
import { Encoding } from std::encoding

pub fn to_b64(s: String) -> String {
    const result = Encoding.btoa(s)
    return result
    crash { Encoding.btoa -> fallback("") }
    test { self("hello") == "aGVsbG8=" }
}
```

| Method | Signature |
|--------|-----------|
| `encode(input)` | `-> Bytes` |
| `decode(bytes)` | `-> String, err { err invalid }` |
| `btoa(input)` | `-> String, err { err invalid }` |
| `atob(input)` | `-> String, err { err invalid }` |

## std::time

Timestamps and date parsing.

```roca
import { Time } from std::time

pub fn timestamp() -> Number {
    return Time.now()
    test {}
}
```

| Method | Signature |
|--------|-----------|
| `now()` | `-> Number` |
| `parse(input)` | `-> Number, err { err parse_failed }` |

## Contracts

### Serializable

For types that can be converted to a JSON string.

```roca
User satisfies Serializable {
    fn toJSON() -> String {
        return '{"name":"' + self.name + '"}'
        test { self() == '{"name":"test"}' }
    }
}
```

### Deserializable\<T\>

For types that can be constructed from a JSON string. Uses generics.

```roca
User satisfies Deserializable<User> {
    fn parse(data: String) -> User, err {
        return User { name: data }
        test { self("cam") is Ok }
    }
}
```

## Inline compilation

When you import from `std::*`, the compiler inlines a thin JS wrapper at the call site. There is no separate runtime file or dependency to install. The compiled output is self-contained.
