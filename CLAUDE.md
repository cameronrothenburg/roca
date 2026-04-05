# CLAUDE.md

This file provides guidance to Claude Code when working with this repository.

## What is Roca

A memory-safe language built for AI. Compiles to JavaScript or native binary. The compiler enforces ownership, runs proof tests natively, and only emits output if everything passes.

## Architecture

Five crates, each with one job:

| Crate | Job | Tests |
|-------|-----|-------|
| **roca-lang** | AST types (26 nodes). Pure data, zero logic. | 9 |
| **roca-mem** | Alloc, own, copy, free. The physical memory model. | 29 |
| **roca-parse** | Tokenize + parse + ownership check + type enforcement. | 60 |
| **roca-native** | AST → Cranelift JIT + proof test execution. | 18 |
| **roca-js** | AST → JavaScript via OXC. | 18 |

```text
.roca source → roca-parse (tokenize → parse → check) → checked AST
                                                          ├─→ roca-native (Cranelift IR → JIT → proof tests)
                                                          └─→ roca-js (OXC JS AST → .js)
```

## Build & Test

```bash
cargo test --release                           # all 134 tests
cargo test --release -p roca-parse             # parse + check tests
cargo test --release -p roca-native            # native JIT tests
cargo test --release -p roca-js                # JS emission tests
cargo test --release -p roca-mem               # memory tests
cargo test --release -p roca-lang              # AST tests
cargo test --release -- test_name              # single test by name
```

## Commit Convention

Conventional commits enforced by commitlint. Scope is **required**. Valid scopes:
`compiler`, `native`, `js`, `spec`, `ci`, `deps`

Example: `feat(compiler): type enforcement — binop and call arg type checking`

## Key Concepts

### Ownership (enforced at parse time)

- `const` = owned. `let` = borrow from const. `var` = mutable owned.
- Parameters: `b` (borrowed) or `o` (owned/consumed)
- Passing to `o` = move. Value is dead after.
- Struct fields always own their values (copy if borrowed)
- Second-class references: borrows cannot be stored or returned

### Types (enforced at parse time)

- Int, Float, String, Bool, Unit, Named (structs), Array, Optional
- Binary ops: both operands must be same type
- Return type must match declared type
- Call args must match param types

### Error Codes

- E-OWN-001 through E-OWN-010: ownership violations
- E-TYP-001/002: type mismatches
- E-STR-006: unknown struct field

### Checker Rules

Pluggable `Rule` trait in `roca-parse/src/rules.rs`. Each rule is a struct implementing `Rule`. Walker calls rules at each AST node. Adding a rule: write a struct, impl Rule, add to `all_rules()`.

## Spec Files

```text
docs/src/spec/syntax.md    — complete syntax reference
docs/src/spec/memory.md    — ownership rules + roca-mem API
docs/src/spec/errors.md    — error code registry
docs/src/spec/feedback.md  — AI teaching messages
```
