---
name: roca-js-crate
description: "JavaScript code generation for roca-js. ALWAYS use this skill when reading, writing, reviewing, or modifying any file in crates/roca-js/. This includes JS emission, .d.ts generation, the error tuple protocol, OXC AST building, and all expression/statement/struct/contract codegen."
---

# roca-js -- JavaScript Code Generation

## Single Responsibility

Translate a fully-checked Roca AST into ES module JavaScript source (`.js`) and TypeScript declaration files (`.d.ts`) using the OXC AST builder and codegen pipeline.

## Boundaries

### Depends On

| Crate | What it provides |
|---|---|
| `roca-ast` | All Roca AST node types (`SourceFile`, `FnDef`, `StructDef`, `ContractDef`, `Expr`, `Stmt`, `CrashBlock`, `TypeRef`, etc.) consumed as read-only input |
| `oxc_allocator` | Arena allocator that owns all OXC AST nodes for the lifetime of a single emit call |
| `oxc_ast` | `AstBuilder` for constructing JS AST nodes (`Function`, `Class`, `Statement`, `Expression`, etc.) |
| `oxc_codegen` | `Codegen` -- serializes the OXC `Program` AST into a JavaScript source string |
| `oxc_span` | `SPAN` sentinel and `SourceType::mjs()` for ES module output |

### Depended On By

`roca-cli` -- calls `emit()` and `emit_dts()` during `roca build` after proof tests pass.

### MUST NOT

- Perform type checking or semantic validation -- the AST is assumed to be fully checked by `roca-check`.
- Parse Roca source -- that is `roca-parse`'s job. (Only used in dev-dependencies for unit tests.)
- Touch Cranelift, JIT, or native compilation -- that is `roca-native` / `roca-cranelift`.
- Implement runtime functions -- those live in `@rocalang/runtime` (npm) and `roca-runtime` (Rust).
- Reject or report user errors -- emit what the checked AST says; trust the checker.

## Key Invariants

### Error Tuple Protocol

Every Roca function that declares `-> T, err` emits JS that returns `{ value, err }` objects instead of throwing. Success: `{ value: T, err: null }`. Failure: `{ value: null, err: { name, message } }`. The `.d.ts` emitter declares `RocaResult<T>` and `RocaError` types when any export uses error returns.

### OXC AST Building Pattern

1. `emit()` creates a fresh `Allocator` and `AstBuilder` per call.
2. Each Roca `Item` dispatches to a module-level builder (`functions::build_function`, `structs::build_struct`, `contracts::build_contract_stmts`, `build_enum`).
3. Builders return OXC `Statement` / `Function` / `Class` nodes, which get pushed into a `Program` body vec.
4. `Codegen::new().build(&program)` serializes the AST to a JS string.
5. Import lines are built as raw strings and prepended (OXC does not own imports in this crate).

### Expression Emission Pipeline

All expression codegen flows through `shapes::expr_to_js`. Each Roca `Expr` variant has a dedicated `js_*` function in `shapes.rs`. Children always recurse through `expr_to_js` -- never bypass the pipeline. To add a new expression shape: add the AST variant in `roca-ast`, add a `js_*` function in `shapes.rs`, wire it into the match in `expr_to_js`.

### Struct Emission

Structs emit as ES classes via `structs::build_struct`. Constructor assigns fields. Struct methods and `satisfies` trait methods are merged into the class body. Satisfies methods are collected in a pre-pass over `Item::Satisfies` and passed in as a slice.

### Contract Emission

Enum-style contracts emit as `const Name = { ... }` objects. Interface contracts with error signatures emit a companion `NameErrors` const with the error map.

### Crash Block Emission

`crash.rs` wraps function calls with try/catch/retry patterns based on the crash strategy kind (`Simple` or `Detailed`). Retry with delay emits `await` and triggers async detection on the enclosing function.

### Async Detection

Functions are automatically marked `async` if `body_has_wait()` or `crash_has_delay()` returns true. There is no explicit async keyword in Roca -- it is inferred from `wait` statements and retry delays.

### Stdlib Detection

A pre-pass collects stdlib imports (`ImportSource::Std`) and reserved extern contracts. If any exist, `import roca from "@rocalang/runtime"` is prepended, and stdlib calls get `roca.` prefixed via a thread-local `STDLIB_CONTRACTS` set in `shapes.rs`.

### .d.ts Generation

`dts.rs` generates TypeScript declarations as raw strings (not OXC AST). It emits `RocaResult<T>` / `RocaError` shared types when error returns exist. Only `pub` items get `export` declarations. Satisfies methods are included on struct types.

## YAGNI Rules

- Do NOT add source maps -- OXC codegen handles that if ever needed upstream.
- Do NOT add minification or tree-shaking -- that is the consumer's bundler.
- Do NOT add bundling or module concatenation.
- Do NOT add runtime polyfills -- those belong in `@rocalang/runtime`.
- Do NOT add formatting/prettifying -- Codegen output is already readable.
- Do NOT build OXC AST for imports -- they are raw strings prepended to output.

## Key Files

| File | Responsibility |
|---|---|
| `lib.rs` | Public API: `emit()`, `emit_dts()`. Pre-pass for satisfies/imports/stdlib. Item dispatch loop. `build_enum`. |
| `functions.rs` | `build_function` -- params, constraint guards, async detection, body emission |
| `structs.rs` | `build_struct` -- class with constructor, methods, satisfies methods |
| `contracts.rs` | `build_contract_stmts` -- enum contracts as const objects, interface error maps |
| `expressions.rs` | Thin wrapper: `build_expr` delegates to `shapes::expr_to_js` |
| `shapes.rs` | Expression pipeline: one `js_*` function per Roca `Expr` variant, stdlib prefixing |
| `statements.rs` | `build_stmt` -- let/const/return/if/for/while/wait/crash handler dispatch |
| `crash.rs` | `wrap_with_strategy` -- retry/fallback/propagate/detailed error matching |
| `dts.rs` | `emit_dts` -- TypeScript declaration file as raw string output |
| `ast_helpers.rs` | Convenience wrappers over OXC's verbose builder API (ident, const_decl, if_stmt, etc.) |
| `helpers.rs` | Shared utilities: `make_result`, `make_error`, `null` constructors |

## Test Patterns

### Unit Tests (in-crate)

`lib.rs` contains `#[cfg(test)] mod tests` with round-trip tests: parse Roca source with `roca-parse`, call `emit()`, assert the JS string contains expected constructs. These verify codegen correctness without running the JS.

```bash
cargo test --release -p roca-js
```

### JS Integration Tests

`tests/js/` contains Bun test suites that compile `.roca` files and execute the emitted JS:

```bash
cd tests/js && bun install
ROCA_BIN=../../target/release/roca bun test
ROCA_BIN=../../target/release/roca bun test compiler.test.js   # single file
```

Key test files: `compiler.test.js` (compilation output), `runtime.test.js` (runtime behavior), `crossfile.test.js` (multi-file imports), `verify.test.js` (proof test verification).

### What to Test When Changing Codegen

1. Add a unit test in `lib.rs::tests` that asserts the emitted JS string.
2. Run existing JS integration tests to catch regressions.
3. For new constructs: add a `.roca` file in `tests/integration/` and a matching JS test.
