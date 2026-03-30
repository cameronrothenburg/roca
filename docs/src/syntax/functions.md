# Functions

Every function has three sections: logic, crash, test.

## Declaration

```roca
fn private_fn(x: Number) -> Number {
    return x * 2
    test { self(3) == 6 }
}

pub fn public_fn(x: Number) -> Number {
    return x * 2
    test { self(3) == 6 }
}
```

- `fn` = private, `pub fn` = exported.
- Every function **must** have a `test {}` block.
- Every call to an error-returning function **must** appear in a `crash {}` block.

## Error-returning functions

Functions that can fail declare `-> Type, err` and list named errors:

```roca
/// Creates a validated account
pub fn create_account(name: String, email: String) -> User, err {
    err invalid_name = "name is required"
    err invalid_email = "email must contain @"

    if name == "" { return err.invalid_name }
    if !email.includes("@") { return err.invalid_email }
    return User { name: name, email: email }

    test {
        self("cam", "cam@test.com") is Ok
        self("", "cam@test.com") is err.invalid_name
        self("cam", "bad") is err.invalid_email
    }
}
```

Error declarations go at the top of the body. Return errors with `return err.name` or override the message with `return err.name("custom message")`.

Error returns include a zero value -- the compiled output returns `("", err)` not `(null, err)`.

## Void return

Use `-> Ok` for functions that return nothing:

```roca
extern fn log(msg: String) -> Ok
```

## Doc comments

Required on all `pub` items:

```roca
/// Single line doc
pub fn greet(name: String) -> String { ... }

/**
 * Multi-line block doc.
 * Explains the function in detail.
 */
pub fn validate(raw: String) -> String, err { ... }
```

## Test blocks

```roca
test {
    self(1, 2) == 3              // assert equality
    self("a@b.com") is Ok        // assert no error
    self("") is err.missing      // assert specific error
}
```

- `self()` calls the enclosing function.
- Every error return must be tested.
- Every function must have at least one success case.
- Async functions are automatically awaited in tests.
- Empty `test {}` is allowed for instance methods.

## Variables

```roca
const x = 5               // immutable
let x = 5                 // mutable, can reassign
self.field = value         // mutate struct fields in methods
```
