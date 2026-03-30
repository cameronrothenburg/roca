# Happy Path

Function bodies contain **only** the success case. This is the core design principle of Roca.

## Two paths, separated by structure

| Section | Purpose |
|---------|---------|
| Function body | What happens when everything works |
| Crash block | What happens when something fails |

The body never sees error variables. Crash blocks intercept errors before they reach your code.

## What this means in practice

- No error variables in the body.
- No `if err` checks.
- No null returns for "not found" cases.
- The crash block decides what happens: `halt`, `fallback`, `retry`, or `panic`.

## Example

```roca
/// Fetches all users from the database
pub fn get_users(db: Database) -> String, err {
    err query_failed = "database query failed"
    const data = wait db.query("SELECT * FROM users")
    return data
    crash {
        db.query -> halt
    }
    test {
        self(__mock_Database) is Ok
    }
}
```

The body reads top-to-bottom as pure success logic. The crash block is a separate declaration of error policy. The test block proves both paths.

## Anti-patterns the compiler rejects

The compiler enforces happy-path purity with two rules:

- **`err-in-body`** -- using `let val, err = call()` to capture errors in the body is rejected. Use a crash block.
- **`manual-err-check`** -- writing `if err { ... }` in the body is rejected. The crash block handles it.
