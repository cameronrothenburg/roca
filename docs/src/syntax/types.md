# Types

## Primitives

| Type | Description |
|------|-------------|
| `String` | Text |
| `Number` | Numeric value |
| `Bool` | `true` or `false` |

## Composite types

| Type | Description |
|------|-------------|
| `Array<T>` | Ordered collection |
| `Map<V>` | Key-value store (string keys) |
| `Optional<T>` | Field that may be absent |
| `Bytes` | Binary data |
| `Buffer` | Writable binary buffer |

## Special types

| Type | Description |
|------|-------------|
| `Ok` | Void return -- function returns nothing |

## Type casts

Safe casts return an error on invalid input:

```roca
let n, err = Number("42")    // returns err on non-numeric string
let s, err = String(42)      // safe cast to string
```

## Enums

```roca
enum Status { active = "active", suspended = "suspended" }
enum HttpCode { ok = 200, not_found = 404 }
```

Compiles to plain JS objects:

```js
const Status = { active: "active", suspended: "suspended" };
```

## Optional<T>

Used for struct fields that may be absent. Never use `Type | null`.

```roca
pub struct Profile {
    name: String
    bio: Optional<String>
}{}
```

Methods on Optional:

| Method | Description |
|--------|-------------|
| `isPresent()` | Returns `Bool` -- whether the value exists |
| `unwrap()` | Returns the value (crashes if absent) |
| `unwrapOr(default)` | Returns the value or the provided default |

## Loggable

Built-in contract requiring `to_log() -> String`. The types `String`, `Number`, `Bool`, and `Bytes` all satisfy `Loggable`. The functions `log()`, `error()`, and `warn()` require `Loggable` arguments.

## Compiler rules

| Rule | Trigger |
|------|---------|
| `type-mismatch` | Comparing different types |
| `struct-comparison` | Comparing structs directly |
| `unknown-method` | Method does not exist on the type |
| `const-reassign` | Reassigning a `const` variable |
| `type-annotation-mismatch` | `let x: Number = "hello"` |
| `not-loggable` | `log`/`error`/`warn` argument missing `to_log()` |
| `return-type-mismatch` | Returning wrong type from function |
| `arg-type-mismatch` | Wrong argument type at call site |
