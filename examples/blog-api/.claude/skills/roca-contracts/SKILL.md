---
name: roca-contracts
description: Roca contracts, structs, generics, extern declarations, satisfies blocks, enums. Use when defining types or implementing contracts.
---

# Roca Contracts & Structs

## Contracts — What Must Be Done

A contract declares signatures, errors, and mocks. No implementation.

```roca
contract Loggable {
    to_log() -> String
}

contract HttpClient {
    get(url: String) -> Response, err {
        err timeout = "request timed out"
        err not_found = "404 not found"
    }
    mock {
        get -> Response { status: 200, body: "{}" }
    }
}
```

## Generic Contracts

Contracts can have type parameters with optional constraints:

```roca
contract Array<T> {
    push(item: T) -> Number
    pop() -> T
    map(callback: T) -> Array
    filter(callback: T) -> Array<T>
    includes(item: T) -> Bool
}

contract Logger<T: Loggable> {
    add(item: T) -> Number
}
```

The compiler enforces:
- `Array<Email>.push(42)` fails — 42 is not Email
- `Logger<Email>` fails if Email doesn't satisfy Loggable

## Structs — How It's Done

A struct has two blocks: first `{}` is the contract (fields + signatures), second `{}` is the implementation.

```roca
pub struct Email {
    value: String
    validate(raw: String) -> Email, err {
        err missing = "required"
        err invalid = "bad format"
    }
}{
    fn validate(raw: String) -> Email, err {
        if raw == "" { return err.missing }
        if !raw.includes("@") { return err.invalid }
        return Email { value: raw }
        crash { raw.includes -> halt }
        test {
            self("a@b.com") is Ok
            self("") is err.missing
            self("bad") is err.invalid
        }
    }
}
```

Struct methods can mutate fields: `self.field = value`

## Satisfies — Linking Struct to Contract

One block per contract. Always separate.

```roca
Email satisfies Loggable {
    fn to_log() -> String {
        return self.value
        test { self() == "test" }
    }
}
```

The compiler checks:
- Every method in the contract is implemented
- Signatures match exactly
- One `satisfies` block per contract

## Field Constraints

Fields can declare inline constraints after the type:

```roca
pub struct User {
    name: String { min: 1, max: 64 }
    email: String { contains: "@", min: 3 }
    age: Number { min: 0, max: 150 }
    bio: String
}{}
```

Available: `min`, `max`, `minLen`, `maxLen`, `contains`, `pattern`.
Compiler rejects: `min > max`, `contains`/`pattern` on Number, any constraint on Bool.

## Extern Declarations

Declare JS runtime types and functions. The compiler type-checks them but emits no JS definitions.

```roca
extern contract HttpClient {
    get(url: String) -> Response, err {
        err timeout = "request timed out"
    }
}

extern contract Response {
    status: Number
    body: String
}

extern fn log(msg: String) -> Ok
```

- `extern contract` — describes a JS shape. Use as struct field types for the adapter pattern.
- `extern fn` — declares a JS function. Emits bare calls. Use for globals or imported functions.
- Mock blocks provide test doubles (externs don't exist at compile time).

**Adapter pattern** — bundle extern contracts into a struct, pass from JS:
```roca
pub struct Runtime { http: HttpClient }{}

pub fn handler(rt: Runtime) -> String, err {
    let resp, err = rt.http.get("/api")
    return resp.body
    crash { rt.http.get -> log |> halt }
}
```

The JS side creates the adapter object and passes it in.

## Enums

```roca
enum Status { active = "active", suspended = "suspended" }
enum HttpCode { ok = 200, not_found = 404 }
```

Compiles to `const Status = { active: "active", suspended: "suspended" };`
