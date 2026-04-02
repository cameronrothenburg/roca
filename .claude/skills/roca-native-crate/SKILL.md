---
name: roca-native-crate
description: "AST-to-Cranelift bridge for roca-native. ALWAYS use this skill when reading, writing, reviewing, or modifying any file in crates/roca-native/. This includes AST compilation, test runner, property tests, and runtime stubs. roca-native translates AST into roca-cranelift API calls — it must never contain raw Cranelift IR."
---

# roca-native — AST-to-Cranelift Bridge

## Single Responsibility

Translate Roca AST nodes into `roca-cranelift` Body/Function API calls, owning all Roca-specific semantics (stdlib dispatch, crash handling, constraint validation, error tuples, inline map/filter) while delegating all IR generation to roca-cranelift.

## Boundaries

### Depends On

| Crate | What roca-native uses |
|---|---|
| **roca-ast** | `SourceFile`, `Item`, `FnDef`, `Expr`, `Stmt`, `TypeRef`, `Field`, `CrashHandlerKind`, `TestBlock`, `Constraint` — the entire AST is read-only input |
| **roca-types** | `RocaType` — used in `NativeCtx.infer_type()`, return-kind maps, and function declarations |
| **roca-cranelift** | `Body`, `Function`, `Value`, `StringPart`, `MatchArmLazy` (the API layer); `JitModule`, `Module`, `FuncId`, `FnDecl`, `CompiledFuncs` (module management); `RuntimeFuncs`, `register_symbols`, `declare_runtime`, `declare_functions` (runtime bridge); `MEM`, `MemTracker` (memory tracking, re-exported for cli) |
| **roca-runtime** | Host function implementations — re-exported through `runtime/mod.rs` for symbol registration |

### Depended On By

**roca-cli** — calls `compile_all()`, `create_jit_module()`, `get_function_ptr()`, `test_runner::run_tests()`, and reads `runtime::MEM`/`MemTracker`.

### MUST NOT

- **No raw Cranelift IR.** Never import `cranelift_codegen`, `cranelift_frontend`, or `cranelift_module` types in `emit/` code. No `ins.*` instructions, no `FunctionBuilder`, no `Variable`, no `Block`, no `AbiParam`. All IR generation goes through `roca-cranelift`'s `Body`/`Function`/`Value` API. (The only exception is `lib.rs::compile_to_object()` which uses `cranelift_object`/`cranelift_native` for the AOT path.)
- **No JS emission.** That belongs in `roca-emit`.
- **No parsing.** `roca-parse` is a dev-dependency only (used in tests via `test_helpers::jit()`).
- **No type checking or static analysis.** That belongs in `roca-check`.
- **No memory management decisions.** Cranelift decides WHEN to free; runtime decides HOW. This crate just calls `Body` methods.

## Key Invariants

### Translation Contract: AST node -> Body method call

Every Roca AST construct maps to 1-3 `Body` method calls. The walker in `emit/emit.rs` has two core functions:

- `emit_body(body, nctx, stmts)` — walks a statement list, calling `emit_stmt` for each
- `emit_expr(body, nctx, expr)` -> `Value` — translates an expression, returning a cranelift Value

Statements map directly: `Stmt::Const` -> `body.const_var_typed()`, `Stmt::If` -> `body.if_else()`, `Stmt::For` -> `body.for_each()`, `Stmt::While` -> `body.while_loop()`, `Stmt::Return` -> `body.return_val()`, etc.

### NativeCtx: Roca-specific metadata alongside Body

`NativeCtx` (in `emit/context.rs`) holds metadata the AST walker needs that the generic `Body` knows nothing about: crash handlers, function return types, enum variants, and struct field definitions. It is passed alongside `Body` to every emit function.

### compile_all() orchestration (in lib.rs)

1. `declare_all_functions()` — forward-declares all function signatures so any function can call any other
2. Compile extern fn stubs and extern contract stubs
3. Compile closures and wait expressions
4. Compile function bodies, struct methods, and satisfies methods

### Test Runner (test_runner.rs)

`run_tests()` creates a JIT module, calls `compile_all()`, finalizes the module, then iterates over `Item::Function` items looking for `test {}` blocks. Each test case calls the JIT-compiled function pointer and checks the return value. Property tests run automatically for public functions with generable parameter types.

### Property Tests (property_tests.rs)

`run_property_tests()` generates 50 rounds of randomized inputs from parameter type signatures and constraints. It verifies: no crash, correct return type, valid error discipline. Only runs for functions where `all_params_generable()` returns true (Number, String, Bool params).

## YAGNI Rules

- **No optimization passes.** roca-cranelift and Cranelift itself handle optimization. This crate just translates.
- **No duplicating cranelift logic.** If Body doesn't have a method you need, add it to roca-cranelift — don't work around it here.
- **No runtime function implementations.** Host functions live in roca-runtime. This crate only registers/calls them.
- **No direct memory tracking.** Re-export `MEM`/`MemTracker` from roca-cranelift for cli; never manipulate allocations directly.

## Key Files

| File | Role |
|---|---|
| `src/lib.rs` | Public API: `compile_all()`, `create_jit_module()`, `get_function_ptr()`, `compile_to_object()`. Orchestrates the full compilation pipeline. |
| `src/emit/mod.rs` | Re-exports from compile, context, emit submodules |
| `src/emit/compile.rs` | Function declaration, metadata extraction (`build_return_kind_map`, `build_enum_variant_map`, `build_struct_def_map`), closure/function/method compilation entry points |
| `src/emit/emit.rs` | The AST walker: `emit_body()`, `emit_stmt()`, `emit_expr()`. Translates every Roca statement and expression into Body API calls. Zero IR imports. |
| `src/emit/context.rs` | `NativeCtx` — Roca-specific compilation state (crash handlers, enum variants, return types, struct defs, type inference) |
| `src/test_runner.rs` | `run_tests()` / `run_tests_only()` — JIT-compiles source, executes inline `test {}` blocks, reports pass/fail |
| `src/property_tests.rs` | `run_property_tests()` — fuzz testing with 50 rounds of randomized inputs from type constraints |
| `src/runtime/mod.rs` | Re-exports from `roca-runtime` and `roca-cranelift` runtime bridge (symbol registration, memory tracker) |
| `src/test_helpers.rs` | `jit(source)` helper that parses + compiles + finalizes in one call; `call_f64()`, `read_native_str()` |

## Test Patterns

Tests live in `src/tests_*.rs` files, organized by domain:

| File | Domain |
|---|---|
| `tests_basic.rs` | Primitives, operators, bindings, strings, function calls |
| `tests_control.rs` | If/else, while, for, break, continue, match |
| `tests_features.rs` | Structs, enums, crash blocks, error tuples, closures |
| `tests_stdlib.rs` | Stdlib method dispatch (string, array, number methods) |
| `tests_stdlib_ext.rs` | Extended stdlib coverage |
| `tests_stdlib_integration.rs` | Cross-cutting stdlib scenarios |
| `tests_integration.rs` | Multi-function, cross-feature integration scenarios |

**Pattern:** Every test uses the `jit(source)` helper to parse Roca source, compile it, and finalize the JIT module. Then it transmutes a function pointer via `call_f64()` and asserts the return value. Tests describe what works correctly (positive outcome names), not what bugs are prevented.

```rust
#[test]
fn return_constant() {
    let mut m = jit("pub fn answer() -> Number { return 42 }");
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "answer")) };
    assert_eq!(f(), 42.0);
}
```

**Dev-dependency:** `roca-parse` is only available in `#[cfg(test)]` — production code receives a pre-parsed `SourceFile` from the caller.
