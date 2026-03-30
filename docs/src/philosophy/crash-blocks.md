# Crash Blocks

Crash blocks declare how each error-returning call is handled. They are separate from the function body, keeping the happy path clean.

## When crash blocks are needed

Only calls to functions that return errors (`-> Type, err`) need crash entries. Stdlib methods like `trim`, `push`, and `split` are safe and need no entry.

## Strategies

Strategies chain with `|>`:

| Strategy | Effect |
|----------|--------|
| `halt` | Propagate the error to the caller |
| `log` | Log the error, continue the chain |
| `retry(n, ms)` | Retry `n` times with `ms` delay between attempts |
| `fallback(value)` | Use a static default value |
| `fallback(fn(e))` | Closure receives error; access `e.name` and `e.message` |
| `panic` | `console.error` + `process.exit(1)` |

## Chaining

```roca
crash {
    db.save -> log |> retry(3, 1000) |> halt
}
```

This logs the error, retries up to 3 times with 1 second delay, and if all retries fail, propagates to the caller.

## Per-error handling

Match on specific error names when a call can fail in multiple ways:

```roca
crash {
    http.get {
        err.timeout -> log |> retry(3, 1000) |> halt
        err.not_found -> fallback("empty")
        default -> log |> halt
    }
}
```

## Error propagation rules

- Chains ending in `halt` -- the caller must declare those errors in its own signature.
- Chains ending in `fallback` or `panic` -- the error is consumed. The caller does not need to declare it.

## Fallback with closures

The closure form receives the error object:

```roca
crash {
    Email.validate -> fallback(fn(e) -> Response.fail(400, e.message))
}
```

## Compiler enforcement

- **`missing-crash`** -- a call to an error-returning function has no crash entry.
- **`unhandled-call`** -- crash block exists but is missing a handler for a specific call.
- **`crash-on-safe`** -- crash entry on a function that does not return errors.
- **`unhandled-error`** -- error propagates via `halt` but the function does not declare it.
- **`panic-warning`** -- `panic` will crash the process; emits a warning suggesting `halt` or `fallback`.
