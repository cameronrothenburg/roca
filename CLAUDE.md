# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Roca

Roca is a contractual programming language that compiles to JavaScript. The compiler validates code, runs proof tests via Cranelift JIT, and only emits JS + `.d.ts` files if all tests pass. Written in Rust.

## Build & Test Commands

```bash
cargo build --release                # build compiler → target/release/roca
cargo test --release                 # all Rust tests
cargo test --release test_name       # single Rust test by name
cargo test --release native::        # tests for a specific module
cargo test --release -- --nocapture  # with stdout output

# JS integration tests
cd tests/js && bun install
ROCA_BIN=../../target/release/roca bun test
ROCA_BIN=../../target/release/roca bun test compiler.test.js  # single file

# CLI smoke test (also run in CI)
./target/release/roca check tests/js/projects/api

# Roca commands
roca build [path]    # check → native proof tests → emit JS
roca check [path]    # parse + type check, no emission
roca test [path]     # build + test, clean output after
roca run [path]      # build + execute via bun
roca repl [--native] # interactive REPL
```

## Commit Convention

Conventional commits enforced by commitlint. Scope is **required**. Valid scopes:
`compiler`, `runtime`, `native`, `checker`, `emitter`, `cli`, `spec`, `ci`, `js`, `deps`

Example: `fix(native): correct heap deallocation for boxed values`

## Compilation Pipeline

```
.roca source → Tokenizer → Parser → AST
  → Static Analysis (14 checker rules)
  → Cranelift JIT (proof test execution)
  → if tests pass: JS Emitter (OXC) → .js + .d.ts
```

## Architecture

### `src/parse/` — Tokenizer & Parser
Recursive descent parser. `tokenizer.rs` produces tokens, `parser.rs` orchestrates, specialized files handle each construct (expr, function, struct, contract, crash, test_block, satisfies, string_interp).

### `src/ast/` — AST Definitions
Node types in `nodes.rs` (top-level items), `types.rs` (type refs), `expr.rs`, `stmt.rs`, `err.rs`, `crash.rs`, `test_block.rs`.

### `src/check/` — Static Analysis
14 rules implementing the `Rule` trait (`src/check/rule.rs`), orchestrated by `walker.rs`. `registry.rs` does a pre-pass to collect contracts/structs before checking. `context.rs` tracks scope/symbols. Rules live in `src/check/rules/` — one file per rule (contracts, structs, satisfies, crash, tests, types, unhandled, manual_err, methods, variables, docs, ownership, reserved, constraints).

### `src/emit/` — JS Code Generation
Uses OXC to build JS AST and emit `.js` + `.d.ts`. Functions emit the error tuple protocol: `{value, err}`. Key files: `functions.rs`, `structs.rs`, `contracts.rs`, `expressions.rs`, `statements.rs`, `crash.rs`, `dts.rs`.

### `src/native/` — Cranelift JIT & Proof Testing
Compiles Roca to Cranelift IR for native test execution. `emit/compile.rs` generates IR, `test_runner.rs` executes inline test blocks, `property_tests.rs` does fuzz testing. `runtime/` provides stdlib stubs for the native runtime. Test suites in `tests_*.rs` files.

### `src/cli/` — CLI Commands
`build.rs` runs the full pipeline, `check.rs` validates without emitting, `config.rs` reads `roca.toml`, `repl.rs` provides interactive mode, `gen_extern.rs` converts `.d.ts` to Roca extern contracts.

### `src/lsp/` — Language Server
Tower-LSP based. Diagnostics, completions, document symbols.

### `src/resolve.rs` — Module Resolution
Recursive import loading, cross-file contract registry building, function signature lookup.

## Key Language Concepts

- **Error tuple protocol**: functions return `{value, err}` instead of throwing
- **Crash blocks**: explicit error handling strategies (halt, retry, fallback, skip, log, panic)
- **Happy path only**: function bodies are the success case; errors go in crash blocks
- **Proof tests required**: every function needs an inline `test {}` block
- **No null**: use `-> Type, err` for failure, `Optional<T>` for absent fields
- **Contracts**: interfaces that structs implement via `satisfies` blocks
- **Extern contracts**: typed wrappers for external JS dependencies, passed as explicit params

## Test Locations

- **Parser**: `src/parse/parser_tests.rs`, `src/parse/expr_tests.rs`
- **Checker**: `src/check/mod.rs` (check_tests module)
- **Native**: `src/native/tests_basic.rs`, `tests_control.rs`, `tests_features.rs`, `tests_integration.rs`, `tests_memory.rs`, `tests_memory_complex.rs`, `tests_stdlib.rs`, `tests_stdlib_integration.rs`, `tests_stdlib_ext.rs`
- **JS integration**: `tests/js/compiler.test.js`, `runtime.test.js`, `crossfile.test.js`, `verify.test.js`
- **Integration .roca files**: `tests/integration/`

## Roca Language Skill

Run `roca man` and `roca patterns` to load the full language manual and coding patterns into context before writing Roca code. Use `roca search <name>` to find stdlib types/methods.
