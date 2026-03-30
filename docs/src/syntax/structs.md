# Structs

Structs have two blocks: the **contract block** (what) and the **implementation block** (how).

## Basic struct

```roca
/// A validated email address
pub struct Email {
    value: String
    validate(raw: String) -> Email, err {
        err missing = "email is required"
        err invalid = "email format is not valid"
    }
}{
    pub fn validate(raw: String) -> Email, err {
        if raw == "" { return err.missing }
        if !raw.includes("@") { return err.invalid }
        return Email { value: raw }
        test {
            self("a@b.com") is Ok
            self("") is err.missing
            self("bad") is err.invalid
        }
    }
}
```

The first `{}` is the contract -- field declarations and method signatures with errors. The second `{}` is the implementation -- actual function bodies with crash and test blocks.

## Field constraints

Constrained fields **must** include a `default` value:

```roca
pub struct User {
    name: String { min: 1, max: 64, default: "unknown" }
    email: String { contains: "@", min: 3, default: "none@none.com" }
    age: Number { min: 0, max: 150, default: 0 }
}{}
```

Available constraints: `min`, `max`, `minLen`, `maxLen`, `contains`, `pattern`, `default`.

The compiler enforces:
- **`missing-default`** -- constrained field has no default value.
- **`invalid-constraint`** -- bad constraint (e.g., `min > max`).

## Optional fields

Use `Optional<T>` for fields that may be absent:

```roca
pub struct Profile {
    name: String
    bio: Optional<String>
}{}
```

## Satisfies

`satisfies` links a struct to a contract — proving the struct implements the contract's methods. This is Roca's approach to type compatibility: anywhere the contract is expected, the struct can be used.

```roca
Email satisfies Loggable {
    fn to_log() -> String {
        return self.value
        test { self() == "test@example.com" }
    }
}
```

Each contract gets its own satisfies block — no chaining:

```roca
// Right — one block per contract
Email satisfies Loggable { ... }
Email satisfies Serializable { ... }

// Wrong — no chaining
Email satisfies Loggable, Serializable { ... }
```

### Why satisfies replaces type aliases

In other languages you might write `type UserId = String`. In Roca, you use satisfies:

```roca
/// A validated user ID
pub struct UserId {
    value: String
    validate(raw: String) -> UserId, err {
        err invalid = "user ID must be alphanumeric"
    }
}{
    pub fn validate(raw: String) -> UserId, err {
        if raw == "" { return err.invalid }
        return UserId { value: raw }
        test {
            self("abc123") is Ok
            self("") is err.invalid
        }
    }
}

UserId satisfies Loggable {
    fn to_log() -> String {
        return self.value
        test {}
    }
}
```

Now `UserId` is a distinct type with validation — not just a string alias. It satisfies `Loggable` so it can be logged. The compiler enforces that every method the contract requires is implemented with the correct signature.

### Swappable implementations

Multiple structs can satisfy the same contract, making them interchangeable:

```roca
/// In-memory cache for development
pub struct MemoryCache {}{}
MemoryCache satisfies Cache {
    fn get(key: String) -> String, err { ... }
    fn set(key: String, value: String) -> Ok, err { ... }
}

/// Redis cache for production
pub struct RedisCache {}{}
RedisCache satisfies Cache {
    fn get(key: String) -> String, err { ... }
    fn set(key: String, value: String) -> Ok, err { ... }
}
```

Both satisfy `Cache`. Your functions accept `Cache` — the caller decides which implementation to use.

## Struct construction

Create struct instances with literal syntax:

```roca
const user = User { name: "cam", email: "cam@test.com", age: 30 }
```

## Compiler rules

| Rule | Trigger |
|------|---------|
| `missing-impl` | Contract method not implemented |
| `sig-mismatch` | Implementation does not match contract signature |
| `satisfies-mismatch` | Satisfies method does not match contract |
| `empty-struct` | Struct has no methods -- use a contract instead |
| `unknown-field` | `self.field` does not exist on the struct |
| `field-type-mismatch` | Wrong field type in struct literal |
