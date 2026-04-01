# Compiler Rules

All rules are hard errors unless noted. Code that violates them does not compile.

## Function rules

| Code | Description |
|------|-------------|
| `missing-test` | Every function needs a test block |
| `missing-crash` | Every error-returning call needs a crash handler |
| `unhandled-call` | Crash block missing a handler for a call |
| `untested-error` | An error return path has no test case |
| `no-success-test` | Error tests exist but no success case |
| `self-referential-test` | Test expected value is a `self()` call — use a concrete value |

## Error handling rules

| Code | Description |
|------|-------------|
| `err-in-body` | Use crash block, not `let val, err = call()` |
| `manual-err-check` | Use crash block, not `if err { ... }` |
| `unhandled-error` | Error propagates via `halt` but caller does not declare it |
| `no-fn-error-def` | Standalone functions cannot define new errors — use a struct |
| `crash-on-safe` | Crash entry on a function that does not return errors |
| `panic-warning` | Warning: `panic` will crash the process |

## Type rules

| Code | Description |
|------|-------------|
| `type-mismatch` | Comparing different types |
| `struct-comparison` | Cannot compare structs directly |
| `unknown-method` | Method does not exist on the type |
| `const-reassign` | Cannot reassign a `const` variable |
| `type-annotation-mismatch` | Declared type does not match assigned value |
| `generic-mismatch` | Wrong type for generic parameter |
| `constraint-violation` | Type does not satisfy generic constraint |
| `not-loggable` | `log`/`error`/`warn` argument missing `to_log()` |

## Return rules

| Code | Description |
|------|-------------|
| `return-type-mismatch` | Returning wrong type from function |
| `return-null` | Returning null from non-nullable function |
| `return-err-not-declared` | Returning error from non-`err` function |

## Null rules

| Code | Description |
|------|-------------|
| `nullable-type` | `Type \| null` used -- use `Optional<T>` or `-> Type, err` |
| `nullable-return` | Function returns `Type \| null` -- use `-> Type, err` |
| `nullable-access` | Calling method on nullable without null check |

## Argument and field rules

| Code | Description |
|------|-------------|
| `arg-type-mismatch` | Wrong argument type at call site |
| `field-type-mismatch` | Wrong field type in struct literal |
| `unknown-field` | `self.field` does not exist on struct |

## Struct and contract rules

| Code | Description |
|------|-------------|
| `missing-impl` | Struct contract method not implemented |
| `sig-mismatch` | Implementation does not match signature |
| `satisfies-mismatch` | Satisfies method does not match contract |
| `empty-struct` | Struct has no methods -- use a contract instead |
| `duplicate-err` | Duplicate error name in contract |

## Constraint rules

| Code | Description |
|------|-------------|
| `missing-default` | Constrained field has no default value |
| `invalid-constraint` | Bad field constraint (e.g., `min > max`) |

## Access rules

| Code | Description |
|------|-------------|
| `private-method` | Calling non-`pub` struct method from outside |
