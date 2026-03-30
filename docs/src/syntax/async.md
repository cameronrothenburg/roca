# Async

Roca has no `async` keyword. Functions that use `wait` automatically become async.

## wait

```roca
const data = wait http.get("/api")
```

Errors from `wait` calls are handled by crash blocks, not try/catch.

## waitAll

Run multiple calls concurrently and collect all results:

```roca
let a, b = waitAll { call1() call2() }
```

## waitFirst

Run multiple calls concurrently and take the first to resolve:

```roca
let fastest = waitFirst { call1() call2() }
```

## Testing async functions

Async functions are automatically awaited in test blocks. No special syntax needed:

```roca
/// Fetches user data
pub fn get_user(db: Database) -> String, err {
    err query_failed = "query failed"
    const data = wait db.query("SELECT * FROM users LIMIT 1")
    return data
    crash { db.query -> halt }
    test {
        self(__mock_Database) is Ok
    }
}
```
