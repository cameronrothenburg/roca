# 5. Error Model

**Status:** Draft

This section defines the Roca error model, including error returns, error declarations, crash blocks, crash strategies, and crash chains. The error model enforces that every possible failure is handled explicitly at the call site.

---

## 5.1 Error Returns

A function that can fail MUST declare `, err` after its return type. Inside the function body, `return err.name` returns a named error to the caller.

```roca
pub fn validate(s: String) -> String, err {
    if s == "" { return err.empty }
    return s
test {
    self("hello") is "hello"
    self("") is err.empty
}
}
```

### 5.1.1 Rules

- The `, err` modifier MUST appear after the return type in the function signature.
- `return err.name` MUST be used to return a named error.
- A function without `, err` MUST NOT use `return err.*` in its body.
- A conforming compiler MUST reject `return err.*` in functions that do not declare `, err`.

---

## 5.2 Error Declarations

Error declarations define the named errors that a function or method can produce. Each declaration binds a name to a human-readable message string.

```roca
get(url: String) -> HttpResponse, err {
    err network = "network error"
    err timeout = "request timed out"
}
```

### 5.2.1 Where Errors Are Declared

- Struct method signatures (in the header block) MAY define error declarations.
- Extern contract method signatures MAY define error declarations.
- Standalone `pub fn` functions MUST NOT define new error names in their signatures. They MAY use `, err` and return errors declared by the types they call.

### 5.2.2 Error Name Rules

- Error names MUST be `lowercase_snake_case`.
- Error names MUST be unique within a single method signature.
- The error message MUST be a string literal.

```roca
pub extern contract Fs {
    readFile(path: String) -> String, err {
        err not_found = "file not found"
        err permission = "permission denied"
        err io = "I/O error"
    }
    writeFile(path: String, data: String) -> Ok, err {
        err permission = "permission denied"
        err io = "I/O error"
    }
}
```

---

## 5.3 Crash Blocks

A crash block defines how errors are handled at the call site. Every call to an error-returning function MUST be covered by a crash handler.

```roca
pub fn fetch_data(url: String) -> String {
    const response = Http.get(url)
    return response
crash {
    Http.get -> log |> retry(3, 1000) |> halt
}
test {
    self("https://example.com") is String
}
}
```

### 5.3.1 Placement

- The `crash` block MUST appear inside the function body, after the main logic and before the `test` block.
- A function MAY have exactly one `crash` block.
- The crash block covers all error-returning calls within the function body.

### 5.3.2 Compiler Enforcement

- A conforming compiler MUST reject any function that calls an error-returning function without a corresponding crash handler (the **missing-crash rule**).
- Every error-returning call in the function body MUST have a matching entry in the crash block.
- A conforming compiler SHOULD produce a clear diagnostic identifying the uncovered call.

---

## 5.4 Crash Strategies

Roca defines six crash strategies. Each strategy specifies a behavior for handling an error.

| Strategy | Syntax | Behavior |
|----------|--------|----------|
| `log` | `log` | Log the error and continue to the next strategy in the chain |
| `halt` | `halt` | Propagate the error to the caller |
| `skip` | `skip` | Swallow the error and continue with `null` or the type's default value |
| `fallback(expr)` | `fallback(0)` | Use the given fallback value instead of the error |
| `retry(n, ms)` | `retry(3, 1000)` | Retry the call `n` times with `ms` milliseconds between attempts |
| `panic` | `panic` | Crash the process immediately |

### 5.4.1 Terminal vs. Non-Terminal Strategies

Strategies are classified as **terminal** or **non-terminal**:

- **Terminal strategies** end the chain and determine the final outcome: `halt`, `fallback`, `skip`, `panic`.
- **Non-terminal strategies** perform an action and pass control to the next strategy: `log`, `retry`.

A crash chain MUST end with a terminal strategy. A conforming compiler MUST reject chains that end with a non-terminal strategy.

### 5.4.2 `log`

The `log` strategy records the error (target-specific mechanism) and continues to the next strategy in the chain.

- `log` is non-terminal — it MUST NOT be the last strategy in a chain.
- The log output format is implementation-defined.

### 5.4.3 `halt`

The `halt` strategy propagates the error to the calling function.

- `halt` is terminal.
- If the enclosing function does not declare `, err`, the compiler MUST reject `halt` — there is nowhere to propagate the error.

### 5.4.4 `skip`

The `skip` strategy swallows the error. Execution continues with `null` (if the receiving type is nullable) or the type's default value.

- `skip` is terminal.
- The receiving binding SHOULD be typed as nullable (`Type | null`) when using `skip`.

### 5.4.5 `fallback(expr)`

The `fallback` strategy replaces the error with a concrete value.

- `fallback` is terminal.
- The expression MUST be type-compatible with the expected return type.

### 5.4.6 `retry(n, ms)`

The `retry` strategy re-executes the failed call up to `n` times, waiting `ms` milliseconds between attempts.

- `retry` is non-terminal — if all retries fail, the next strategy in the chain handles the error.
- `n` MUST be a positive integer literal.
- `ms` MUST be a non-negative integer literal.

### 5.4.7 `panic`

The `panic` strategy terminates the process.

- `panic` is terminal.
- A conforming implementation MUST log the error before terminating.
- `panic` SHOULD be used only as a last resort — prefer `halt` or `fallback` in most cases.

---

## 5.5 Crash Chains

Crash strategies are composed into chains using the pipe operator `|>`. Strategies execute left to right.

```roca
crash {
    Http.get -> log |> retry(3, 1000) |> halt
}
```

In this example:

1. `log` — log the error.
2. `retry(3, 1000)` — retry up to 3 times with 1-second delay.
3. `halt` — if all retries fail, propagate the error.

### 5.5.1 Chain Rules

- A chain MUST contain at least one strategy.
- A chain MUST end with a terminal strategy (`halt`, `fallback`, `skip`, or `panic`).
- Non-terminal strategies (`log`, `retry`) MUST NOT appear as the final strategy.
- Terminal strategies MUST NOT appear in non-final positions — they end the chain.

### 5.5.2 Chain Syntax

```
CrashEntry  = CallTarget "->" CrashChain
CrashChain  = Strategy ("|>" Strategy)*
Strategy    = "log"
            | "halt"
            | "skip"
            | "panic"
            | "fallback" "(" Expr ")"
            | "retry" "(" Number "," Number ")"
```

The `CallTarget` identifies the error-returning function call. It MUST match a call that appears in the function body. The syntax is `TypeOrModule.methodName`.

---

## 5.6 Detailed Crash Handlers

A crash entry MAY use a block form to handle individual error names separately.

```roca
crash {
    Fs.readFile {
        err.not_found -> fallback("default content")
        err.permission -> halt
        default -> log |> halt
    }
}
```

### 5.6.1 Per-Error Handling Rules

- Each `err.name` entry specifies a chain for that specific error.
- The `default` entry handles any error not explicitly listed.
- If a detailed crash handler is used, a `default` entry SHOULD be provided.
- A conforming compiler SHOULD warn if a detailed handler omits `default` and does not cover all declared errors.

### 5.6.2 Syntax

```
DetailedCrash   = CallTarget "{" ErrorHandler+ "}"
ErrorHandler    = "err." name "->" CrashChain
                | "default" "->" CrashChain
```

### 5.6.3 Example with Multiple Calls

A crash block MAY contain multiple entries — one per error-returning call.

```roca
pub fn copy_file(src: String, dest: String) -> Ok {
    const content = Fs.readFile(src)
    Fs.writeFile(dest, content)
    return Ok
crash {
    Fs.readFile {
        err.not_found -> halt
        default -> log |> halt
    }
    Fs.writeFile -> log |> retry(2, 500) |> halt
}
test {
    self("/tmp/a.txt", "/tmp/b.txt") is Ok
}
}
```

---

## 5.7 Fallback with Closure

The `fallback` strategy MAY accept a closure instead of a simple expression. The closure receives the error object as its argument.

```roca
crash {
    Http.get -> fallback(fn(e) -> ApiResponse.fail(500, e.message))
}
```

### 5.7.1 Rules

- The closure MUST accept exactly one parameter: the error object.
- The closure's return type MUST be compatible with the expected type at the call site.
- The error object provides at minimum a `message` field of type `String`.

### 5.7.2 Simple vs. Closure Fallback

| Form | Syntax | Use case |
|------|--------|----------|
| Simple | `fallback(0)` | Static default value |
| Closure | `fallback(fn(e) -> compute(e))` | Dynamic value derived from the error |

```roca
// Simple fallback — use a static default
crash {
    Config.load -> fallback(Config.defaults())
}

// Closure fallback — construct a response from the error
crash {
    Http.get -> fallback(fn(e) -> Response { status: 500, body: e.message })
}
```
