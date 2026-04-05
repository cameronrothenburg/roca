# Error Codes

All compiler diagnostics grouped by domain. See [AI Feedback Loop](./feedback.md) for teaching messages.

---

## Ownership Errors (E-OWN)

Enforced by the checker during parsing. See [Memory Model](./memory.md) for the rules.

| Code | Rule | Condition |
|------|------|-----------|
| `E-OWN-001` | const owns | Value created without a `const` owner |
| `E-OWN-002` | let borrows | `let` creates a new value instead of borrowing from `const` |
| `E-OWN-003` | borrow before pass | `const` passed directly to a `b` parameter without `let` |
| `E-OWN-004` | use after move | Value used after being passed to an `o` parameter |
| `E-OWN-005` | declare intent | Parameter missing `o` or `b` qualifier |
| `E-OWN-006` | return owned | Function returns a borrowed non-primitive value |
| `E-OWN-007` | container copy | Borrowed value copied into container (note, not error) |
| `E-OWN-009` | branch symmetry | Value consumed in one `if` branch but not the other |
| `E-OWN-010` | loop consumption | Owned value consumed in loop without reassignment |

## Type Errors (E-TYP)

| Code | Condition |
|------|-----------|
| `E-TYP-001` | Type mismatch — return type, binary op operands, or call argument wrong |
| `E-TYP-002` | Unknown type name |

## Struct Errors (E-STR)

| Code | Condition |
|------|-----------|
| `E-STR-006` | Unknown field on struct |

---

## Error Handling

No crash blocks. Errors are handled inline:

```roca
let result, err = call()
if err { return "" }
```

Built-in helpers:
- `retry(n, ms, fn)` — retry a closure
- `fallback(result, default)` — use default on error
