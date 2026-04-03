# 7. Error Codes

This section is the central registry for all compiler diagnostics. Error codes are grouped by domain. Each code has a short description here — see [AI Feedback Loop](./feedback.md) for the full teaching messages with fix suggestions.

---

## 7.1 Ownership Errors (E-OWN)

These enforce the memory model defined in [Section 5](./memory.md).

| Code | Rule | Condition |
|------|------|-----------|
| `E-OWN-001` | const owns | Value created without a `const` owner |
| `E-OWN-002` | let borrows | `let` binding creates a new value instead of borrowing from a `const` |
| `E-OWN-003` | borrow before pass | `const` passed directly to a `b` parameter without a `let` |
| `E-OWN-004` | use after move | Value used after being passed to an `o` parameter |
| `E-OWN-005` | declare intent | Function parameter missing `o` or `b` qualifier |
| `E-OWN-006` | return owned | Function returns a borrowed value instead of an owned one |
| `E-OWN-007` | container copy | Borrowed value copied into container (note, not error) |
| `E-OWN-008` | second-class ref | Reference stored in struct field or returned from function |
| `E-OWN-009` | branch symmetry | Owned value consumed in one `if` branch but not the other |
| `E-OWN-010` | loop consumption | Owned value consumed in loop without reassignment |

---

## 7.2 Syntax Errors (E-SYN)

These enforce the grammar defined in [Section 2](./syntax.md).

| Code | Condition |
|------|-----------|
| `E-SYN-001` | Unexpected token — expected `{expected}`, got `{actual}` |
| `E-SYN-002` | Unterminated string literal |
| `E-SYN-003` | Invalid number literal |
| `E-SYN-004` | Missing return type — every function must declare `-> Type` |
| `E-SYN-005` | Missing test block — every `pub fn` must have a `test {}` block |
| `E-SYN-006` | Empty struct — struct must have at least one field or method |
| `E-SYN-007` | Invalid import path — must be relative (`./`) with `.roca` extension |
| `E-SYN-008` | Duplicate parameter name |
| `E-SYN-009` | Reserved keyword used as identifier |

---

## 7.3 Type Errors (E-TYP)

These enforce the type system defined in [Section 3](./types.md).

| Code | Condition |
|------|-----------|
| `E-TYP-001` | Type mismatch — expected `{expected}`, got `{actual}` |
| `E-TYP-002` | Unknown type name |
| `E-TYP-003` | Wrong number of type arguments |
| `E-TYP-004` | Constraint violation — value does not satisfy `{constraint}` |
| `E-TYP-005` | Cannot compare structs with `==` (use field comparison) |
| `E-TYP-006` | Nullable access — cannot call method on `Optional<T>` without checking |

---

## 7.4 Error Handling Errors (E-ERR)

These enforce that all errors are handled explicitly.

| Code | Condition |
|------|-----------|
| `E-ERR-001` | Unhandled error — call to error-returning function without `let val, err =` |
| `E-ERR-002` | Error not checked — `err` variable declared but never read |
| `E-ERR-003` | Return `err.*` in function without `, err` return type |
| `E-ERR-004` | Unknown error name — `return err.name` where `name` is not declared |
| `E-ERR-005` | Missing error declaration — function returns `, err` but declares no error names |

---

## 7.5 Struct Errors (E-STR)

These enforce struct rules defined in [Section 3.4](./types.md#34-structs).

| Code | Condition |
|------|-----------|
| `E-STR-001` | Missing method implementation — signature declared but no body |
| `E-STR-002` | Signature mismatch — implementation params/return don't match signature |
| `E-STR-003` | Undeclared method — implementation without a matching signature |
| `E-STR-004` | Private method called from outside the struct |
| `E-STR-005` | Unknown method — type does not have method `{name}` |
| `E-STR-006` | Unknown field — struct does not have field `{name}` |

---

## 7.6 Contract Errors (E-CON)

These enforce contract and satisfies rules.

| Code | Condition |
|------|-----------|
| `E-CON-001` | Missing satisfies implementation — contract method not implemented |
| `E-CON-002` | Satisfies mismatch — param count or return type doesn't match contract |
| `E-CON-003` | Unknown contract name |

---

## 7.7 Module Errors (E-MOD)

These enforce import and cross-file resolution.

| Code | Condition |
|------|-----------|
| `E-MOD-001` | Import not found — file does not exist at path |
| `E-MOD-002` | Name not exported — imported name is not `pub` in source file |
| `E-MOD-003` | Circular import detected |

---

## 7.8 Error Handling: What Replaces Crash Blocks

Crash blocks are removed from the language. Error handling is explicit inline code using `let val, err =` and stdlib helpers.

### Pattern: Check and return

```roca
pub fn load(b path: String) -> Config, err {
    err not_found = "config not found"

    let data, failed = Fs.readFile(path)
    if failed { return err.not_found }

    const config = parse(data)
    return config
}
```

### Pattern: Retry

```roca
const data = retry(3, 1000, fn() -> Http.get(url))
```

`retry(attempts, delay_ms, fn)` is a stdlib function. It calls the closure up to `attempts` times, waiting `delay_ms` between tries. Returns the first success or the last error.

### Pattern: Fallback

```roca
const config = fallback(load_config(path), Config.default())
```

`fallback(result, default)` is a stdlib function. If `result` is an error, returns `default`. Otherwise returns the value.

### Pattern: Log and continue

```roca
let result, failed = db.query(sql)
if failed {
    log("query failed: " + failed.message)
}
```

### Why no crash blocks

Crash blocks were special syntax for something that's just code. `retry`, `fallback`, and `log` are functions — they don't need their own grammar. Inline `let val, err =` is already in the language. The six crash strategies are replaced by:

| Old crash strategy | New pattern |
|-------------------|-------------|
| `halt` | `if failed { return err.name }` |
| `skip` | `if failed { }` (ignore) |
| `fallback(expr)` | `const x = fallback(call(), default)` |
| `retry(n, ms)` | `const x = retry(n, ms, fn() -> call())` |
| `log` | `if failed { log(failed.message) }` |
| `panic` | `if failed { panic(failed.message) }` |
