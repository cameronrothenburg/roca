# 6. Test Model

This section defines the test model for Roca programs. Every public function carries its own proof. The compiler enforces test presence, shape correctness, and error coverage before any code ships.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

---

## 6.1 Test Blocks

Every function that takes parameters and transforms them MUST contain a `test` block. This applies to all `pub fn` declarations. A conforming compiler MUST reject any `pub fn` that lacks a test block with diagnostic `missing-test`.

Functions that are non-deterministic (e.g., calling extern contracts with no params, returning random values) are exempt from mandatory testing — the compiler cannot verify their output.

```roca
/// Takes params, transforms them — MUST have tests
pub fn add(a: Number, b: Number) -> Number {
    return a + b
test {
    self(2, 3) == 5
    self(0, 0) == 0
    self(-1, 1) == 0
}}
```

### 6.1.1 The `self` Keyword

Inside a test block, the keyword `self` refers to the enclosing function. Calling `self(args)` invokes the function with the given arguments. The `self` keyword MUST NOT appear outside of a test block. A conforming compiler MUST reject `self` in any other context with diagnostic `self-outside-test`.

```roca
pub fn clamp(value: Number, low: Number, high: Number) -> Number {
    return value
test {
    self(5, 0, 10) == 5
    self(-3, 0, 10) == 0
    self(15, 0, 10) == 10
}}
```

### 6.1.2 Test Block Placement

The test block MUST appear after all executable statements in the function body. It MUST be the last item before the closing brace. A function body MUST contain exactly one test block -- zero or more than one is a compile error.

---

## 6.2 Assertion Types

Test blocks support three assertion forms. Each assertion occupies one line.

A test block MUST contain:
- At least one **happy path** assertion (`== value` or `is Ok`) proving the function works with valid input
- One **error path** assertion (`is err.name`) for EVERY declared error, proving each failure case is reachable

| Syntax | Meaning | Usage |
|--------|---------|-------|
| `self(args) == value` | Return value equals expected | Success cases with concrete output |
| `self(args) is Ok` | Function succeeds (no error) | Success without checking the exact value |
| `self(args) is err.name` | Function returns specific error | Error path coverage |

### 6.2.1 Equality Assertions

An equality assertion calls `self` with arguments and compares the return value to an expected value using structural equality. For structs, all fields MUST match. For arrays, all elements MUST match in order and length.

The expected value MUST NOT be a `self(...)` call. Comparing `self(args)` against `self(other_args)` proves nothing about correctness — the function is compared against itself. A conforming compiler MUST reject this with diagnostic `self-referential-test`.

```roca
pub fn double(n: Number) -> Number {
    return n * 2
test {
    self(0) == 0
    self(5) == 10
    self(-3) == -6
}}
```

### 6.2.2 Ok Assertions

An `is Ok` assertion verifies that the function completes without returning an error. It MUST only be used on functions that declare errors (`, err` in the return type). Using `is Ok` on a function that cannot return errors is a compile error (`ok-on-infallible`).

```roca
pub fn save(name: String) -> String, err {
    err empty_name = "name must not be empty"
    return name
test {
    self("alice") is Ok
    self("") is err.empty_name
}}
```

### 6.2.3 Error Assertions

An `is err.name` assertion verifies that the function returns the named error for the given inputs. The error name MUST match one of the errors declared in the function body. Referencing an undeclared error is a compile error (`unknown-error-name`).

```roca
pub fn divide(a: Number, b: Number) -> Number, err {
    err division_by_zero = "cannot divide by zero"
    return a / b
test {
    self(10, 2) == 5
    self(0, 1) == 0
    self(1, 0) is err.division_by_zero
}}
```

---

## 6.3 Test Shape Checking

The compiler MUST verify that every equality assertion's expected value is type-compatible with the function's return type. A mismatch MUST produce a compile error with diagnostic `test-shape-mismatch`.

```roca
// COMPILE ERROR: test-shape-mismatch
// Function returns Number, but assertion expects String
pub fn add(a: Number, b: Number) -> Number {
    return a + b
test {
    self(2, 3) == "five"   // ERROR: expected Number, got String
}}
```

Shape checking rules:

- **Number**: Expected value MUST be a number literal.
- **String**: Expected value MUST be a string literal.
- **Bool**: Expected value MUST be `true` or `false`.
- **Struct**: Expected value MUST be a struct literal with all required fields matching their declared types.
- **Array**: Expected value MUST be an array literal where each element matches the array's element type.
- **Enum**: Expected value MUST be a valid enum variant (e.g., `Color.Red`).
- **Optional**: Expected value for `Optional<T>` return types is implementation-defined.

```roca
pub struct Point {
    x: Number
    y: Number
}

pub fn origin() -> Point {
    return Point { x: 0, y: 0 }
test {
    self() == Point { x: 0, y: 0 }
}}
```

---

## 6.4 Error Test Coverage

If a function returns errors, the compiler MUST enforce complete coverage:

1. **Every declared error MUST be tested.** For each error the function can return, there MUST be at least one `self(args) is err.name` assertion proving that error path is reachable. A missing error test MUST produce diagnostic `untested-error`.

2. **At least one success case MUST exist.** A function with error declarations MUST have at least one assertion that proves the happy path works (`self(args) == value` or `self(args) is Ok`). A test block with only error assertions MUST produce diagnostic `no-success-test`.

```roca
// COMPILE ERROR: untested-error (missing test for err.too_long)
pub fn validate(input: String) -> String, err {
    err too_short = "input must be at least 3 characters"
    err too_long = "input must be at most 100 characters"
    return input
test {
    self("hello") == "hello"
    self("ab") is err.too_short
    // Missing: self("a long string...") is err.too_long
}}
```

```roca
// COMPILE ERROR: no-success-case
pub fn parse(input: String) -> Number, err {
    err not_a_number = "input is not a valid number"
    return 0
test {
    self("abc") is err.not_a_number
    // Missing: at least one success assertion
}}
```

---

## 6.5 Deep Property Testing

Deep property tests are compiler-generated adversarial test cases that run automatically alongside user-written tests. They verify that a `pub fn` never crashes for any valid input. Property tests run natively via the Cranelift JIT as part of `roca check` and `roca build`.

### 6.5.1 Scope

Property tests run for every `pub fn` and `pub` struct method whose parameters are all generable types (`Number`, `String`, `Bool`). Functions with struct, contract, or generic params are skipped.

### 6.5.2 Input Generation

The compiler auto-generates randomized inputs based on parameter types:

| Type | Generated Values |
|------|-----------------|
| `Number` | `0`, `-1`, `1`, `0.5`, `-0.5`, `NaN`, `Infinity`, `-Infinity`, `MAX_SAFE_INTEGER`, `MIN_SAFE_INTEGER`, random floats |
| `String` | `""`, `" "`, `"a"`, long strings (64 chars), XSS attempts, escape characters, numeric strings, keyword-like strings, random ASCII |
| `Bool` | `true`, `false` |

### 6.5.3 Constrained Parameter Probing

For parameters with declared constraints (e.g., `min`, `max`, `minLen`, `maxLen`), the compiler MUST generate boundary-probing values:

- The boundary value itself (`min`, `max`, `minLen`, `maxLen`)
- One value outside the boundary (`min - 1`, `max + 1`)
- A midpoint value (`(min + max) / 2`)
- Random values within the valid range

For `contains` constraints, the compiler generates strings that both match and don't match the substring.

### 6.5.4 Invariants

Each generated input set MUST verify:

1. **No crash** — the function returns without panicking.
2. **Type correctness** — the return value is the declared type.
3. **Error discipline** — if the function returns `, err`, the error tag is either Ok or a valid error index.

### 6.5.5 Execution

Property tests run automatically — no flags or configuration needed. The compiler generates 50 input combinations per function using a deterministic PRNG seeded from the function name (reproducible failures).

Output uses the `◆` marker to distinguish from explicit test cases:

```text
  ✓ clamp(5, 0, 10) == 5
  ✓ clamp(-1, 0, 10) == 0
  ◆ clamp: 50 property tests passed
  ◆ Validator.check: 50 property tests passed (12 returned errors)
```

A property test failure blocks JS emission, just like explicit test failures.

---

## 6.6 Auto-Stubs

Functions that depend on `extern` contracts receive auto-generated stubs during test execution. The compiler derives stub return values from the contract's type signatures. No `mock` block is needed.

### 6.6.1 Default Return Values

The compiler MUST generate stubs using the following default values:

| Return Type | Default Stub Value |
|-------------|-------------------|
| `String` | `""` |
| `Number` | `0` |
| `Bool` | `false` |
| `[T]` | `[]` |
| `Struct` | Struct with all fields set to their type defaults |
| `Optional<T>` | `null` |
| `Ok` (no return value) | `null` |

### 6.6.2 Stub Behavior

When a function under test calls an extern contract method, the stub MUST be invoked instead of any real implementation. The stub MUST:

1. Accept any arguments without validation.
2. Return the default value for the declared return type.
3. Never throw an exception.
4. Never produce side effects.

```roca
extern contract Db {
    fn get(id: String) -> User, err
    fn save(user: User) -> Ok, err
}

// During tests, db.get returns a User with default fields,
// and db.save returns null (Ok default)
pub fn findUser(db: Db, id: String) -> User, err {
    err not_found = "user does not exist"
    const user = db.get(id)
    return user
test {
    self(db, "123") is Ok
    self(db, "") is err.not_found
}}
```

### 6.6.3 Stub Override

A conforming implementation MAY support explicit stub overrides in test blocks for cases where default values are insufficient. The syntax and semantics of stub overrides are implementation-defined and are not specified in this version of the spec.

---

## 6.7 Test Execution

### 6.7.1 Native Test Engine

Tests run natively via the Cranelift JIT compiler. The compiler IS the test runner — there is no separate JS test harness.

```bash
roca build              # compile + test natively → emit JS if pass
roca test               # test only, no JS output
roca test path/file.roca  # test a specific file
```

The test engine MUST:
- Compile all functions to native code via Cranelift JIT
- Execute each test block assertion
- Execute battle tests with generated inputs
- Report pass/fail with function name, input arguments, expected vs actual on failure
- Block JS emission if any test fails

### 6.7.2 Test-Then-Emit Guarantee

The compiler proves code correctness natively before emitting JS. If all tests pass on the native engine, the emitted JS is guaranteed correct by construction. The JS output is never tested directly — the native engine is the single source of truth.

### 6.7.3 Test Isolation

Each test assertion MUST execute in isolation. Side effects from one assertion MUST NOT affect subsequent assertions. A conforming implementation MUST ensure that:

- Each `self(args)` call gets fresh auto-stubs.
- Mutable state (if any via `let` bindings) is reset between assertions.
- No shared state leaks between test assertions within a test block or across test blocks.
