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

Link a struct to a contract with `satisfies`. Each contract gets its own block:

```roca
Email satisfies Loggable {
    fn to_log() -> String {
        return self.value
        test { self() == "test@example.com" }
    }
}
```

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
