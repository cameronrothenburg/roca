# Safety by Compilation

Roca doesn't trust the developer — human or AI. Safety is structural, enforced at compile time. No JS is emitted until the compiler proves the code is correct.

## What the compiler guarantees

Every Roca build enforces these invariants:

| Guarantee | How |
|-----------|-----|
| Every error is handled | Crash blocks required for error-returning calls |
| Every function is tested | Proof tests run before JS is emitted |
| No unhandled errors leak | Fuzz testing with random inputs |
| No null in user code | `nullable-type` and `nullable-return` rules |
| Types are checked | Return types, argument types, field types validated |
| No secret leaks via logging | `log()` requires the `Loggable` contract |
| Public API is documented | `missing-doc` rule on pub items |
| Struct methods have correct signatures | `sig-mismatch`, `missing-impl` rules |

If any of these fail, the compiler exits with an error and **no JavaScript is written**.

## Guard creation

When a function declares `-> Type, err`, the compiler creates guards automatically:

```roca
/// Validates an email address
pub fn validate(raw: String) -> Email, err {
    err missing = "email is required"
    err invalid = "email format is not valid"

    if raw == "" { return err.missing }
    if !raw.includes("@") { return err.invalid }
    return Email { value: raw }

    test {
        self("a@b.com") is Ok
        self("") is err.missing
        self("bad") is err.invalid
    }
}
```

The compiled JS wraps the return in `{value, err}`:

```js
function validate(raw) {
    if (raw === "") return { value: null, err: { name: "missing", message: "email is required" } };
    if (!raw.includes("@")) return { value: null, err: { name: "invalid", message: "email format is not valid" } };
    return { value: new Email({ value: raw }), err: null };
}
```

The caller's crash block then unwraps:

```js
const _tmp = validate(raw);
const _err = _tmp.err;
if (_err) throw _err;          // halt — propagate
const email = _tmp.value;      // success — use the value
```

This is generated automatically. The Roca developer never writes error handling code in the function body.

## Validation logic

The compiler enforces validation at multiple levels:

### 1. Type validation
Every assignment, return, and argument is type-checked:

```roca
/// Greeting function
pub fn greet(name: String) -> Number {
    return name  // COMPILE ERROR: function returns Number but got String
    test { self("cam") == 0 }
}
```

### 2. Null validation
Null cannot leak into non-nullable code:

```roca
/// Find a user — WRONG
pub fn find(id: String) -> User | null {  // COMPILE ERROR: nullable-return
    ...
}

/// Find a user — RIGHT
pub fn find(id: String) -> User, err {
    err not_found = "user not found"
    ...
}
```

### 3. Error completeness
Every error path must be tested. Every error-returning call must be handled:

```roca
/// Validates input
pub fn validate(s: String) -> String, err {
    err empty = "empty"
    err invalid = "invalid"

    if s == "" { return err.empty }
    if s == "bad" { return err.invalid }
    return s

    test {
        self("ok") == "ok"
        self("") is err.empty
        // COMPILE ERROR: untested-error — 'invalid' is not tested
    }
}
```

### 4. Field constraints with defaults
Constrained fields must declare what happens when the constraint isn't met:

```roca
/// A user account
pub struct User {
    name: String { min: 1, max: 64, default: "unknown" }
    age: Number { min: 0, max: 150, default: 0 }
}{}
```

Without the `default`, the compiler rejects the code with `missing-default`.

## Fuzz testing

After proof tests pass, the compiler fuzzes every public function with random inputs. This catches error paths the developer didn't think of.

The fuzzer generates edge cases per type:

| Type | Fuzz values |
|------|-------------|
| String | `""`, `" "`, `"a" * 1000`, `"<script>"`, `"null"`, `"\n\t\r"` |
| Number | `0`, `-1`, `MAX`, `MIN`, `0.1 + 0.2` |
| Bool | `true`, `false` |

If a fuzz input causes an uncaught exception, the build fails:

```
FAIL: validate[fuzz:3] with ("<script>", "-1") — missing error path. Add crash block or declare -> Type, err
```

This means there's a code path that throws without a crash handler. The developer must either:
- Add a crash block with `fallback` or `halt`
- Declare the function as `-> Type, err` and add error returns

Functions with non-primitive parameters (structs, extern contracts) are automatically excluded from fuzzing — only functions with `String`, `Number`, `Bool` params are fuzzed.

## Crash block validation

The compiler validates crash blocks themselves:

| Rule | What it catches |
|------|----------------|
| `crash-on-safe` | Crash entry on a function that doesn't return errors |
| `panic-warning` | Using `panic` (warns: "this will crash the process") |
| `missing-crash` | Error-returning call with no crash handler |
| `unhandled-call` | Crash block missing a handler for an error-returning call |

```roca
/// Process data
pub fn process(db: Database) -> String {
    const data = wait db.query("SELECT *")
    return data
    crash {
        db.query -> halt           // OK — db.query returns errors
        data.trim -> fallback("")  // COMPILE ERROR: crash-on-safe — trim doesn't return errors
    }
    test { self(__mock_Database) is Ok }
}
```

## The safety chain

1. **Parse** — syntax errors caught
2. **Check** — 50+ rules validate types, errors, null, docs, constraints
3. **Build** — JS emitted only if checks pass
4. **Proof test** — inline tests must pass
5. **Fuzz test** — random inputs must not crash
6. **Output** — JS + .d.ts with full type safety

If any step fails, no output. The developer sees the exact error with context.
