---
name: roca-rules
description: Roca language rules — functions, tests, crash blocks, variables, control flow, closures, async, extern, enums. Use when writing or reviewing Roca code.
---

# Roca Language Rules

## Functions

Every function has three sections: logic, crash, test.

```roca
fn name(params) -> ReturnType {
    // logic — the happy path
    crash { /* error handlers */ }
    test { /* proof */ }
}
```

- `fn` = private, `pub fn` = exported
- Every function MUST have a `test {}` block
- Every function call MUST appear in the `crash {}` block
- Functions that can fail return `value, err`

## Test Blocks

```roca
test {
    self(1, 2) == 3          // assert equality
    self("a@b.com") is Ok    // assert no error
    self("") is err.missing   // assert specific error
}
```

- `self()` calls the function being tested
- Tests can ONLY call `self()` — nothing else
- Empty `test {}` is allowed for instance methods (tested via integration)
- Async functions (with `wait`) are automatically awaited in tests

## Crash Blocks

Crash blocks declare how each call's errors are handled. Strategies chain with `|>`:

```roca
crash {
    name.trim -> halt                                // propagate to caller
    db.save -> log |> retry(3, 1000) |> halt         // log, retry, then propagate
    analytics -> log |> skip                          // log and swallow
    config.load -> panic                              // crash the process
    fetch -> fallback("default")                      // use default value
    http.get {                                        // per-error handling
        err.timeout -> log |> retry(3, 1000) |> halt
        err.not_found -> fallback("empty")
        default -> log |> halt
    }
}
```

Steps: `log`, `retry(n, ms)`, `halt`, `skip`, `fallback(value)`, `panic`

- Chains ending in `halt` → error propagates, caller must handle it
- Chains ending in `fallback`/`skip`/`panic` → error is consumed
- `halt` on `let val, err = call()` in `, err` functions auto-returns `[zero, err]`

## Variables

- `const x = 5` — immutable, cannot reassign
- `let x = 5` — mutable, can reassign
- `let result, err = fn()` — destructure error tuple
- `self.field = value` — mutate struct fields in methods

## Errors

Errors are contracts with name + message:
```roca
err missing = "value is required"
```

Returns `{ name: "missing", message: "value is required" }`. Override the message at return:
```roca
return err.missing("name cannot be blank")
// { name: "missing", message: "name cannot be blank" }
```

Error returns include a zero value for the type: `("", err)` not `(null, err)`.
Tests match on `.name`: `self("") is err.missing`.
Errors are scoped per-method — different methods can reuse error names.

## Visibility

- `fn` / `struct` = private (not exported)
- `pub fn` / `pub struct` = exported

## Control Flow

- `if condition { } else { }` — conditional
- `for item in items { }` — iteration
- `while condition { }` — loops with `break` and `continue`
- `match value { pattern => result, _ => default }` — pattern matching
- Match arms can return errors: `match x { 200 => "ok", 404 => err.not_found, _ => err.unknown }`

## Closures

```roca
items.map(fn(x) -> x * 2)
items.filter(fn(x) -> x > 5)
const double = fn(x) -> x * 2
```

## Async (wait)

```roca
let data, failed = wait http.get(url)
let a, b, failed = waitAll { call1() call2() }
let fastest, failed = waitFirst { call1() call2() }
```

Functions with `wait` auto-become async. No `async` keyword needed.

## Null

- `null` is a keyword — use explicitly
- `Type | null` makes a field/param nullable
- Method calls on nullable values require a null check first

## Type Casts

```roca
let n, err = Number("42")   // safe — returns err on invalid
let s, err = String(42)     // safe — null returns err
```

## Enums

```roca
enum Status { active = "active", suspended = "suspended" }
```

Compiles to a const object: `const Status = { active: "active", suspended: "suspended" };`
