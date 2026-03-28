# Roca — Language Design

A contractual language for AI-written code. The compiler enforces, the AI writes intent. Output is trusted TypeScript.

## Primitives

```
String          "hello"
Number          42, 3.14
Bool            true, false
```

No `any`. No `null`. No `undefined`. No casting. Every value traceable to its type.

## Variables

```
const name = "hello"           // immutable forever
let count = 0                  // mutable binding
count = count + 1              // OK
name = "world"                 // COMPILE ERROR: const
```

## Functions

```roca
fn helper(x: Number) -> Number {
    return x + 1

    test {
        helper(0) == 1
        helper(-1) == 0
    }
}

pub fn greet(name: String) -> String {
    return "Hello " + name

    test {
        greet("cam") == "Hello cam"
        greet("") == "Hello "
    }
}
```

- Every function must have a `test` block — no exceptions
- `pub` makes it public, default is private
- The compiler auto-generates additional fuzz tests from input types

## Types

Types are closed contracts. A type defines what it is and what it can do. Nothing else is allowed.

```roca
pub type Email from String {
    to_string() -> String { return self }

    validate(raw: String) -> Email, err {
        if !raw.contains("@") { return err("missing @") }
        if raw.len() < 3 { return err("too short") }
        return Ok

        test {
            Email("cam@test.com") is Ok
            Email("nope") is err
            Email("") is err
        }
    }
}
```

- Types built from primitives with `from`
- Mandatory `validate` constructor — no value exists without passing validation
- Operations must be defined explicitly — `to_string()`, operators, etc.
- If the compiler can't prove a value is the type, it won't compile

### Explicit operations

Nothing is implicit. `log()` takes `String`. To log anything else, define `to_string()`:

```roca
pub type Secret from String {
    to_string() -> String {
        return "REDACTED"
    }
}

log(secret.to_string())   // "REDACTED"
log(secret)                // COMPILE ERROR: log takes String
```

Operators must be defined per type:

```roca
pub type Price from Number {
    add(other: Price) -> Price { return Price(self + other) }
    to_string() -> String { return "$" + self.raw().to_string() }
    // no subtract, no multiply — not defined, compile error if used
}

let total = priceA + priceB    // calls Price.add — works
let bad = priceA * priceB      // COMPILE ERROR: multiply not defined on Price
```

### Closed enums

Types can define a fixed set of values:

```roca
pub type StatusCode {
    200
    201
    400
    404
    500

    to_string() -> String { ... }
}

res.send(200, "ok")        // fine
res.send(418, "teapot")   // COMPILE ERROR: 418 is not a StatusCode
```

## Structs

```roca
pub type User {
    name: String
    email: Email
    age: Number

    validate(name: String, email: String, age: Number) -> User, err {
        let e, err = Email(email)
        if name.len() == 0 { return err("name required") }
        if age < 0 { return err("invalid age") }
        return Ok

        test {
            User("cam", "cam@test.com", 25) is Ok
            User("", "cam@test.com", 25) is err
            User("cam", "bad", 25) is err
            User("cam", "cam@test.com", -1) is err
        }
    }

    to_string() -> String {
        return self.name
    }
}
```

If it exists, it's valid. No invalid state anywhere in the program.

## Traits

Types implement traits to define behavior. The compiler checks: does the impl exist? Yes → call it. No → error.

```roca
trait Loggable {
    log() -> String
}

trait Serializable {
    serialize() -> String
}

pub type Email from String {
    impl Loggable {
        log() -> String { return self }
    }
    impl Serializable {
        serialize() -> String { return self }
    }
}

pub type Secret from String {
    impl Loggable {
        log() -> String { return "REDACTED" }
    }
    // no Serializable — compile error if you try to serialize
}
```

## Mock blocks

Contracts that represent external systems declare a `mock` block. Mocks must satisfy every contract in the chain — no bare objects, no empty values. The compiler validates mocks the same way it validates real code.

```roca
contract HttpClient {
    get(url: String) -> Response {
        err timeout = "request timed out"
        err not_found = "404 not found"
        err server_error = "500 internal error"
    }

    post(url: String, body: String) -> Response {
        err timeout = "request timed out"
        err server_error = "500 internal error"
    }

    mock {
        get -> Response {
            status: StatusCode.200
            body: Body.validate("{}")
        }
        post -> Response {
            status: StatusCode.201
            body: Body.validate("{}")
        }
    }
}

contract FileSystem {
    read(path: String) -> String {
        err not_found = "file not found"
        err denied = "permission denied"
    }

    write(path: String, data: String) -> Ok {
        err full = "disk full"
        err denied = "permission denied"
    }

    mock {
        read -> "mock content"
        write -> Ok
    }
}
```

- Error states defined on the contract with named errors
- Mock block provides happy-path defaults
- The compiler auto-swaps mocks during battle testing
- Mocks must satisfy every contract in the chain. If `Response` has a `StatusCode` and a `Body`, the mock provides a real `StatusCode.200` and a real `Body.validate("{}")`. Contracts all the way down.

## Crash blocks

Functions define a `crash` block to handle failures. Happy path at top, error handling at bottom. Erlang philosophy — let it crash, but choose how.

```roca
pub fn poll_prices(http: HttpClient, db: Database) -> Ok {
    let response = http.get("https://api.prices.com/latest")
    let prices = response.body.validate(PriceList)
    db.save(prices)
    return Ok

    crash {
        http.get {
            timeout -> retry(3, 1000)
            404 -> fallback(empty_list)
            500 -> retry(1, 5000)
            default -> halt
        }
        validate -> skip
        db.save -> retry(1, 500)
    }
}
```

### Strategies

- `retry(n, ms)` — try n times, wait ms between attempts
- `skip` — ignore the failure, move on
- `halt` — stop, propagate error to caller
- `fallback(val)` — use a default value

### Rules

- Every failable call must have a crash handler
- Crash handlers can target specific error codes or use `default`
- The compiler tests every error state against its handler
- If you miss a failable call — compile error

## Test blocks

Every function must include a `test` block. Inline, right in the function.

```roca
pub fn handle_hello(req: Request, res: Response) -> Response {
    let user = req.body.validate(User)
    return res.send(200, "Hello " + user.name.to_string())

    crash {
        validate -> res.send(400, err.to_string())
    }

    test {
        200 {
            mock req.body -> '{"name": "cam"}'
        }
        400 {
            mock req.body -> 'invalid'
        }
    }
}
```

### Rules

- Every function must have a test block — no exceptions
- For handlers: every return status must have a test mock
- If tests fail, no TypeScript is emitted

### Contract fuzz testing

The compiler also fuzzes every function with random inputs — but not to test logic. To test **contract completeness**.

Every error a function can produce must either be named in a contract and handled in a crash block, or the compiler rejects it. The fuzzer proves nothing outside the contracts can happen.

```roca
pub fn process(email: String, db: Database) -> User, err {
    let e = Email.validate(email)
    let user = User { name: "test", email: e }
    db.save(user)
    return user

    crash {
        Email.validate -> halt
        db.save -> retry(1, 500)
    }

    test {
        self("a@b.com", db) is Ok
        self("bad", db) is err.invalid
    }
}
```

The compiler throws random strings at `email`. Two outcomes are valid:

1. Returns a value that satisfies the return contract → pass
2. Returns a named error handled by a crash block → pass

Anything else — an uncontracted error leaking through — is a compile error:

```
error: process threw "RangeError" when given input "\u0000\u0000..."
  → this error is not in any contract and has no crash handler
```

Every error is either named in a contract and handled, or the compiler rejects it. No surprises in production.

## Visibility

```roca
fn internal()       // private — only this file
pub fn api()        // public — other modules can import
```

- Default is private
- `pub` is the only modifier — simple

## Error handling

Functions that can fail return `value, err`:

```roca
fn divide(a: Number, b: Number) -> Number, err {
    if b == 0 { return err("division by zero") }
    return a / b

    test {
        divide(10, 2) == 5
        divide(10, 0) is err
    }
}
```

Callers handle errors in the `crash` block, not inline.

## Control flow

```
if condition {
    // ...
} else {
    // ...
}

for item in items {
    // ...
}

match value {
    pattern => result
    _ => default
}
```

## What Roca doesn't have

No `null`, `undefined`, `any`, `void`, `class`, `this`, `new`, `async`, `await`, `try`, `catch`, `throw`, `typeof`, `instanceof`.

- `null/undefined` → types with `validate`, errors are explicit
- `any` → doesn't exist, every value has a provable type
- `class` → types with methods
- `async/await` → compiler handles it, transparent to the developer
- `try/catch/throw` → crash blocks with strategies
- `typeof` → types are known at compile time

## Project structure

```
roca/
  src/
    compiler/
      mod.rs              // pipeline: parse → check → emit
    keywords/
      fn.rs               // token, parse, rules, emit, test
      let.rs
      const.rs
      type.rs
      pub.rs
      trait.rs
      crash.rs
      mock.rs
      test.rs
    types/
      string.rs
      number.rs
      bool.rs
    operators/
      add.rs
      compare.rs
```

Adding a keyword = one file. Adding a type = one file. Everything about a feature lives together.
