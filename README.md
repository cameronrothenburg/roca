# Roca

A contractual language. AI writes intent, the compiler enforces contracts, output is trusted TypeScript.

## Why

AI writes code well. It just doesn't write *safe* code. It skips validation, forgets error handling, logs secrets, returns bare objects. Not because it's dumb — because nothing stops it.

Roca stops it. Every value has a contract. Every call has error handling. Every function has tests. If the compiler can't prove it's correct, no TypeScript gets emitted.

## Concepts

Roca has five building blocks:

| Keyword | Purpose |
|---|---|
| `contract` | Defines what must be done. Signatures, errors, mocks. No implementation. |
| `struct` | Implements logic. First `{}` is its contract, second `{}` is implementation. |
| `satisfies` | Links a struct to a contract. One block per contract, never chained. |
| `crash` | Error handling for every function call. Defined per call, per error code. |
| `test` | Inline proof. On every function. Uses `self()`. Mandatory. |

## Quick example

```roca
// Contract — what must be done to be Stringable
contract Stringable {
    to_string() -> String
}

// Contract — what an HTTP client looks like
contract HttpClient {
    get(url: String) -> Response {
        err timeout = "request timed out"
        err not_found = "404 not found"
        err server_error = "500 internal error"
    }

    mock {
        get -> Response {
            status: StatusCode.200
            body: Body.validate("{}")
        }
    }
}

// Struct — implementation with its own contract
pub struct Email {
    value: String

    validate(raw: String) -> Email, err {
        err missing = "value is required"
        err invalid = "format is not valid"
    }
}{
    validate(raw: String) -> Email, err {
        if raw.len() == 0 { return err.missing }
        if !raw.contains("@") { return err.invalid }
        return Email { value: raw }

        crash {
            raw.len -> halt
            raw.contains -> halt
        }

        test {
            self("") is err.missing
            self("nope") is err.invalid
            self("a@b.com") is Ok
        }
    }
}

// Satisfies — Email can be turned into a String
Email satisfies Stringable {
    to_string() -> String {
        return self.value

        test {
            self() == "a@b.com"
        }
    }
}

// Function — logic with crash handling and tests
pub fn greet(name: String) -> String {
    let trimmed = name.trim()
    return "Hello " + trimmed

    crash {
        name.trim -> halt
    }

    test {
        self("cam") == "Hello cam"
        self(" cam ") == "Hello cam"
    }
}
```

## Contracts

A contract declares what something must do. No implementation — just signatures, errors, and mocks.

```roca
contract Serializable {
    serialize() -> String
    deserialize(raw: String) -> Self, err {
        err invalid = "format is not valid"
        err missing = "value is required"
    }
}
```

Contracts can declare:
- Method signatures
- Named error states with messages (`err name = "message"`)
- Mock blocks for testing
- Fixed value sets (enums)

### Enums

```roca
contract StatusCode {
    200
    201
    400
    404
    500
}
```

Use as `StatusCode.200`. Any other value is a compile error.

## Structs

A struct has two blocks. First `{}` is the contract — fields and method signatures. Second `{}` is the implementation — function bodies, crash blocks, and tests.

```roca
pub struct Price {
    amount: Number

    add(other: Price) -> Price
    to_string() -> String
}{
    add(other: Price) -> Price {
        return Price { amount: self.amount + other.amount }

        test {
            self(Price{amount: 5}).amount == 15
        }
    }

    to_string() -> String {
        return "$" + self.amount.to_string()

        crash {
            self.amount.to_string -> halt
        }

        test {
            self() == "$10"
        }
    }
}
```

If a method isn't in the first `{}`, it can't be called. The contract defines what exists.

## Satisfies

Links a struct to a contract. Always a separate block. Always one contract at a time. Never chained.

```roca
// Good — separate blocks
Email satisfies Stringable {
    to_string() -> String { ... }
}

Email satisfies Serializable {
    serialize() -> String { ... }
    deserialize(raw: String) -> Email, err { ... }
}

// This does NOT exist — no chaining
Email satisfies Stringable, Serializable { ... }  // COMPILE ERROR
```

The compiler checks each block independently: does the struct implement every method the contract requires? Missing one → compile error. Wrong signature → compile error.

### Swappable implementations

```roca
pub struct BunHttp {}{}
BunHttp satisfies HttpClient {
    get(url: String) -> Response { ... }
}

pub struct DenoHttp {}{}
DenoHttp satisfies HttpClient {
    get(url: String) -> Response { ... }
}
```

Both satisfy `HttpClient`. Both interchangeable. The contract guarantees it.

## Errors

Errors are named and defined in contracts. Code references them with `err.name`.

### Defining errors

```roca
contract Validatable {
    validate(raw: String) -> Self, err {
        err missing = "value is required"
        err invalid = "format is not valid"
    }
}
```

### Returning errors

```roca
if raw.len() == 0 { return err.missing }
if !raw.contains("@") { return err.invalid }
```

The compiler checks: does `err.missing` exist in the contract? No → compile error. No magic strings.

### Testing errors

```roca
test {
    self("") is err.missing
    self("nope") is err.invalid
    self("a@b.com") is Ok
}
```

## Crash blocks

Every function call must have a crash handler. The crash block lives inside the function, after the logic.

```roca
pub fn save_user(name: String, email: String, db: Database) -> Ok {
    let e = Email.validate(email)
    let user = User { name: name, email: e }
    db.save(user)
    return Ok

    crash {
        Email.validate -> halt
        db.save {
            err.timeout -> retry(3, 1000)
            err.duplicate -> skip
            default -> halt
        }
    }

    test {
        self("cam", "a@b.com", db) is Ok
        self("cam", "bad", db) is err.invalid
    }
}
```

### Strategies

| Strategy | What it does |
|---|---|
| `retry(n, ms)` | Try n times, wait ms between attempts |
| `skip` | Ignore the failure, move on |
| `halt` | Propagate the error to the caller as-is |
| `fallback(val)` | Use a default value |

### Specific error handling

Crash blocks can handle specific errors from the contract:

```roca
crash {
    http.get {
        err.timeout -> retry(3, 1000)
        err.not_found -> fallback(empty)
        err.server_error -> retry(1, 5000)
        default -> halt
    }
}
```

### Why every call?

The crash block doubles as documentation. Glance at it and you see every dependency the function has and what happens when each one fails. Nothing hidden.

## Test blocks

Every function must have a `test` block. Inline. Mandatory.

### Rules

- `self()` calls the function being tested
- `self.method()` calls the struct method being tested
- Tests can **only** call self — nothing else
- The compiler auto-generates additional fuzz tests from input types
- Tests fail → no TypeScript emitted

```roca
fn add(a: Number, b: Number) -> Number {
    return a + b

    test {
        self(1, 2) == 3
        self(0, 0) == 0
        self(-1, 1) == 0
    }
}
```

### Contract fuzz testing

The compiler also fuzzes every function with random inputs — not to test logic, but to test **contract completeness**. Every error must either be named in a contract and handled in a crash block, or the compiler rejects it. If a random input produces an uncontracted error, that's a compile error. No surprises in production.

### Handler tests

For functions that return different statuses, each status needs a mock setup:

```roca
test {
    StatusCode.200 {
        mock req.body -> Body.validate('{"name": "cam"}')
    }
    StatusCode.400 {
        mock req.body -> Body.validate('invalid')
    }
}
```

## Mock blocks

Contracts for external systems declare a `mock` block. The compiler uses it to auto-test code that depends on them.

```roca
contract Database {
    save(data: String) -> Ok {
        err connection_lost = "connection lost"
        err duplicate = "duplicate key"
        err timeout = "request timed out"
    }

    mock {
        save -> Ok
    }
}
```

### Mocks validate contracts all the way down

No bare objects. No empty values. If a `Response` has a `StatusCode` and a `Body`, the mock provides a real `StatusCode` and a real `Body`:

```roca
mock {
    get -> Response {
        status: StatusCode.200
        body: Body.validate("{\"name\": \"mock\"}")
    }
}
```

The compiler validates mocks against the same contracts as real code. If `Body.validate` rejects the mock value, the mock itself won't compile. Mocks are not shortcuts — they must satisfy every contract in the chain, just like real code does.

## Visibility

```roca
fn internal()          // private — only this file
pub fn api()           // public — importable

struct Local {}{}      // private
pub struct Api {}{}    // public
```

Default is private. `pub` makes it importable. That's it.

## What Roca doesn't have

No `null`, `undefined`, `any`, `void`, `class`, `this`, `new`, `async`, `await`, `try`, `catch`, `throw`.

| Instead of | Roca uses |
|---|---|
| `null` / `undefined` | Explicit `value, err` returns |
| `any` | Doesn't exist. Every value has a provable type. |
| `class` | Structs satisfying contracts |
| `async` / `await` | Compiler handles it transparently |
| `try` / `catch` / `throw` | Crash blocks with strategies |
| `typeof` | Types known at compile time |
