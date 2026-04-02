---
name: roca-check-crate
description: "Static analysis rules and walker for roca-check. ALWAYS use this skill when reading, writing, reviewing, or modifying any file in crates/roca-check/. This includes checker rules, the walker, the Rule trait, context/scope tracking, and the contract registry."
---

# roca-check -- Static Analysis

## Single Responsibility

Walks the Roca AST and runs 15 pluggable rules to produce diagnostics, without emitting code or executing anything.

## Boundaries

### Depends On

- **roca-ast** -- AST node types (`SourceFile`, `Item`, `FnDef`, `Expr`, `Stmt`, `TypeRef`, `Field`)
- **roca-errors** -- `RuleError` diagnostic type returned by every rule
- **roca-resolve** -- `ContractRegistry` for cross-file contract/struct lookup

### Dev-Depends On

- **roca-parse** -- used only in `#[cfg(test)]` to parse source strings into `SourceFile`

### Depended On By

- roca-cli (build/check commands)
- roca-lsp (live diagnostics)

### MUST NOT

- Import or reference `roca-cranelift`, `roca-native`, or any Cranelift types -- no codegen
- Import or reference `roca-emit` or OXC -- no JS emission
- Perform I/O (file reads, network) -- the walker receives a pre-parsed `SourceFile`
- Execute user code or proof tests -- that is `roca-native`'s job
- Mutate the AST -- analysis is read-only

## Key Invariants

1. **Rule trait contract** -- Every rule implements `rule::Rule` with four optional hooks: `check_item`, `check_function`, `check_stmt`, `check_expr`. All hooks have default empty implementations (`vec![]`); a rule only overrides the hooks it needs.

2. **Walker dispatch order** -- `walker::walk` iterates items in source order. For each item it calls `check_item` on all rules first, then descends into functions/methods. For each function it calls `check_function`, then walks statements top-to-bottom calling `check_stmt` and recursing into expressions via `check_expr`.

3. **Scope tracking** -- The walker builds a `Scope` (`HashMap<String, VarInfo>`) as it walks. `Const`/`Let` statements insert entries with inferred or annotated types. Struct fields are pre-loaded as `self.<field>` keys. Scope is cloned at branches (if/else) for narrowing (e.g. nullable types after null checks).

4. **Type inference** -- `infer_type_with_registry` resolves expression types using scope + registry. Constructor calls (`Name(...)`) infer the type from the callee name. Method calls check the registry for return types, with hardcoded String method returns as fallback.

5. **Rule registration** -- All rules are instantiated in `lib.rs::all_rules()`. Adding a new rule requires: (a) a new file in `rules/`, (b) a `pub mod` in `rules/mod.rs`, (c) a `Box::new(...)` entry in `all_rules()`.

6. **Context hierarchy** -- Contexts nest: `CheckContext` (file + registry) > `ItemContext` > `FnCheckContext` > `StmtContext` / `ExprContext`. Each deeper context borrows its parent. `FnContext` carries `qualified_name` (e.g. `Email.validate`) and optional `parent_struct`.

## YAGNI Rules

- **No auto-fix suggestions** -- rules return diagnostics only, not code transforms
- **No codegen-aware checks** -- do not inspect or predict emitted JS; that is the emitter's concern
- **No duplicate type resolution** -- use `walker::resolve_type` and `infer_type_with_registry` instead of reimplementing type lookup in individual rules
- **No runtime behavior checks** -- do not simulate execution; property testing belongs in `roca-native`
- **No direct file I/O in rules** -- cross-file info comes pre-resolved via `ContractRegistry`

## Key Files

| File | Purpose |
|---|---|
| `src/lib.rs` | Public API (`check`, `check_with_registry`, `check_with_registry_and_dir`), `all_rules()` registration, `check_tests` module |
| `src/rule.rs` | `Rule` trait with four hooks: `check_item`, `check_function`, `check_stmt`, `check_expr` |
| `src/walker.rs` | AST traversal, scope building, type inference helpers (`resolve_type`, `infer_type_with_registry`, `type_ref_to_name`) |
| `src/context.rs` | Context structs (`CheckContext`, `ItemContext`, `FnCheckContext`, `StmtContext`, `ExprContext`), `VarInfo`, `Scope` type alias |
| `src/rules/mod.rs` | Re-exports all 15 rule modules |
| `src/rules/contracts.rs` | Validates contract definitions |
| `src/rules/constraints.rs` | Validates generic constraints |
| `src/rules/structs.rs` | Validates struct definitions and fields |
| `src/rules/satisfies.rs` | Checks satisfies blocks match their contract |
| `src/rules/crash.rs` | Validates crash block structure |
| `src/rules/tests.rs` | Enforces every function has a `test {}` block |
| `src/rules/variables.rs` | Const reassignment, unused variables |
| `src/rules/methods.rs` | Method call validation and type checking |
| `src/rules/types.rs` | Type mismatch and return type checks |
| `src/rules/unhandled.rs` | Detects unhandled error returns |
| `src/rules/manual_err.rs` | Forbids manual error construction |
| `src/rules/docs.rs` | Enforces doc comments on public items |
| `src/rules/ownership.rs` | Ownership and move tracking |
| `src/rules/reserved.rs` | Rejects reserved identifier names |
| `src/rules/self_test.rs` | Validates self-test patterns in test blocks |

## Test Patterns

Tests live in `src/lib.rs` inside `#[cfg(test)] mod check_tests`. The pattern is:

1. Parse a Roca source string with `roca_parse::parse(r#"..."#)` to get a `SourceFile`.
2. Call `check(&file)` to run all rules.
3. Assert on the returned `Vec<RuleError>` -- either `errors.is_empty()` for valid programs, or `errors.len() >= N` / check specific error messages for invalid programs.

```rust
#[test]
fn valid_program_passes_all_checks() {
    let file = parse::parse(r#"..."#);
    let errors = check(&file);
    assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
}
```

To test a single rule in isolation, instantiate it directly and call its hook with a hand-built context, but the current convention is integration-style: parse full source and assert on aggregated diagnostics.
