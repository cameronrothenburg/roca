# No Null

Roca has no `null` in user code. The compiler rejects nullable types and nullable returns.

## For functions that might fail

Use `-> Type, err` and declare named errors. The caller handles them in a crash block.

**Wrong:**

```roca
pub fn find(id: String) -> User | null {    // compiler error: nullable-return
    ...
}
```

**Right:**

```roca
/// Finds a user by ID
pub fn find(id: String) -> User, err {
    err not_found = "user not found"
    ...
}
```

The caller decides the policy:

```roca
const user = find(id)
crash { find -> fallback(fn(e) -> default_user) }
```

## For struct fields that may be absent

Use `Optional<T>`.

**Wrong:**

```roca
pub struct Profile {
    bio: String | null      // compiler error: nullable-type
}{}
```

**Right:**

```roca
pub struct Profile {
    name: String
    bio: Optional<String>
}{}
```

Access optional fields with methods:

```roca
if profile.bio.isPresent() { ... }
const text = profile.bio.unwrapOr("No bio")
```

## Compiler rules

| Rule | Trigger |
|------|---------|
| `nullable-type` | `Type \| null` used anywhere |
| `nullable-return` | Function returns `Type \| null` |
| `return-null` | Returning `null` from a non-nullable function |
| `nullable-access` | Calling a method on a nullable value without a null check |
