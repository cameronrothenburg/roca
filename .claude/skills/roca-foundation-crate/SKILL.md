---
name: roca-foundation-crate
description: "Foundation leaf crates: roca-ast, roca-errors, roca-types. ALWAYS use this skill when reading, writing, reviewing, or modifying any file in crates/roca-ast/, crates/roca-errors/, or crates/roca-types/. These are dependency-free crates that every other crate imports — changes here ripple across the entire compiler."
---

# Foundation Crates — AST, Errors, Types

## Single Responsibility

- **roca-ast**: AST node definitions — the data structures the parser produces and every stage consumes. Also owns language constants (keywords, built-in types, reserved names) in `constants.rs`.
- **roca-errors**: Error codes (`&str` constants) and diagnostic types (`RuleError`, `ParseError`) shared across all crates. Single source of truth for every diagnostic code the checker emits.
- **roca-types**: Semantic type model (`RocaType` enum) plus typed representations of params, fields, constraints, crash blocks, test blocks, and function signatures. Provides `From<AST>` conversions in `convert.rs`.

## Boundaries

### Depends On

These are leaf crates with minimal dependencies:
- **roca-ast** — zero internal deps. Zero external deps.
- **roca-errors** — zero internal deps. Zero external deps.
- **roca-types** — depends on **roca-ast** only (for `TypeRef`, `Expr`, crash/test AST nodes used in `From` conversions).

### Depended On By

| Crate | roca-ast | roca-errors | roca-types |
|---|---|---|---|
| roca-parse | yes | yes | - |
| roca-check | yes | yes | - |
| roca-resolve | yes | - | - |
| roca-js (emitter) | yes | - | - |
| roca-native | yes | - | yes |
| roca-cranelift | - | - | yes |
| roca-cli | yes | yes | - |
| roca-lsp | yes | yes | - |
| roca (root bin) | yes | yes | yes |

### MUST NOT

- Import any non-leaf crate (no roca-check, roca-parse, roca-native, etc.).
- Contain logic, validation, traversal, or IO. Pure data definitions only.
- Allocate heap memory beyond what derives require (`Clone`, `Debug`).
- Use `std::fs`, `std::io`, `std::net`, or any side-effecting APIs.
- Add external dependencies without explicit approval — these crates compile in seconds and must stay that way.

## Key Invariants

1. **AST is the shared language.** All node types (`Item`, `Expr`, `Stmt`) are enums. Every consumer must handle every variant — exhaustive `match` arms are enforced by the compiler. Never use `#[non_exhaustive]` on these enums.
2. **Error codes are `&str` constants, not enum variants.** This avoids import churn when adding codes — consumers only import what they use. Codes follow the pattern `"kebab-case"` (e.g., `MISSING_CRASH = "missing-crash"`).
3. **`RocaType` mirrors the language type system.** Variants: `Number`, `Bool`, `Void`, `String`, `Array(T)`, `Map(K,V)`, `Struct(name)`, `Enum(name)`, `Optional(T)`, `Fn(params, ret)`, `Unknown`. The `Unknown` variant is a sentinel for unresolved types — it must never appear in emitted output.
4. **`TypeRef` (AST) vs `RocaType` (semantic).** `TypeRef` is what the parser produces from source syntax. `RocaType` is the resolved type used by checker and backends. The conversion lives in `roca-types/src/convert.rs` via `From<&TypeRef>`.
5. **Crash and test blocks exist in both AST and types.** The AST versions (`roca_ast::CrashBlock`, `roca_ast::TestBlock`) hold raw `Expr` nodes. The types versions (`roca_types::CrashBlock`, `roca_types::TestBlock`) are semantic mirrors used by the native backend.

## YAGNI Rules

- Do NOT add visitor/walker traits — walkers belong in consumers (roca-check has `walker.rs`).
- Do NOT add serde/serialization derives unless a concrete feature requires it.
- Do NOT add `Display` impls for AST nodes — `Debug` is sufficient for diagnostics. (`RuleError` and `ParseError` already have `Display`; that is the exception.)
- Do NOT add convenience methods that belong in consumers (e.g., "find all calls in an expr" belongs in the checker, not on `Expr`).
- Do NOT add builder patterns — AST nodes are constructed directly by the parser.
- Do NOT add `Default` impls — AST nodes have no meaningful defaults.

## Key Files

### roca-ast (`crates/roca-ast/src/`)
| File | Contents |
|---|---|
| `lib.rs` | Module declarations and re-exports (`SourceFile`, `Item`, `Expr`, `Stmt`, `TypeRef`, etc.) |
| `nodes.rs` | Top-level items: `SourceFile`, `Item`, `ContractDef`, `StructDef`, `FnDef`, `EnumDef`, `ImportDef`, `SatisfiesDef`, `ExternFnDef` |
| `expr.rs` | `Expr` enum (14 variants: String, Number, Bool, Ident, BinOp, Call, FieldAccess, StructLit, Array, Index, Match, StringInterp, Not, Await, Closure, Null, SelfRef, EnumVariant), `BinOp`, `MatchArm`, `StringPart`, helper fns `expr_to_dotted_name`, `call_to_name` |
| `stmt.rs` | `Stmt` enum, `WaitKind`, `collect_returned_error_names` |
| `types.rs` | `TypeRef` enum — source-level type references |
| `crash.rs` | `CrashBlock`, `CrashHandler`, `CrashHandlerKind`, `CrashArm`, `CrashChain`, `CrashStep` |
| `test_block.rs` | `TestBlock`, `TestCase` |
| `err.rs` | `ErrDecl` — named error declarations |
| `constants.rs` | `KEYWORDS`, `BUILTIN_TYPES`, reserved names — single source of truth for tokenizer and LSP |

### roca-errors (`crates/roca-errors/src/`)
| File | Contents |
|---|---|
| `lib.rs` | All error code constants (40+ `&str` consts), `RuleError` struct (code + message + context), `ParseError` struct (message + token pos) |

### roca-types (`crates/roca-types/src/`)
| File | Contents |
|---|---|
| `lib.rs` | `RocaType` enum + methods (`is_heap`, `is_primitive`, `is_nullable`, `element_type`, `base_name`, `unwrap_optional`), `Param`, `Field`, `Constraint` enum, `ErrDecl`, `FnSignature`, `CrashBlock`/`CrashHandler`/`CrashStep`, `TestBlock`/`TestCase` |
| `convert.rs` | `From<&TypeRef> for RocaType`, `From<&ast::Param> for Param`, `From<&ast::CrashBlock> for CrashBlock`, `From<&ast::TestCase> for TestCase`, etc. |

## Ripple Rules

### Adding a new AST node variant (e.g., new `Item` or `Expr` variant)
1. Add the variant in `roca-ast/src/nodes.rs` or `expr.rs`.
2. **Compiler will catch all missing match arms.** Fix every exhaustive match in:
   - `roca-parse` — parser must produce it.
   - `roca-check` — walker/rules must handle it.
   - `roca-js` — JS emitter must emit it (or explicitly skip).
   - `roca-native` — native backend must compile it (or explicitly skip).
   - `roca-lsp` — document symbols, completions may need updating.
3. If the node carries types, add a `From` conversion in `roca-types/src/convert.rs`.
4. Run `cargo build --release` — zero warnings policy means every unhandled arm is a build error.

### Adding a new error code
1. Add the `pub const` in `roca-errors/src/lib.rs` following the section grouping (crash, contract, struct, satisfies, test, variable, type, method, etc.).
2. Use it in the relevant checker rule in `roca-check/src/rules/`.
3. No other crates need changes — error codes are consumed by string, not by enum match.

### Adding a new `RocaType` variant
1. Add the variant to the `RocaType` enum in `roca-types/src/lib.rs`.
2. Update every method on `RocaType`: `is_heap`, `is_primitive`, `base_name`, and any others.
3. Add `From<&TypeRef>` mapping in `convert.rs` if there is a corresponding `TypeRef`.
4. **Compiler will catch all missing match arms** in consumers: `roca-cranelift` (IR generation), `roca-native` (JIT compilation), `roca-check` (type checking), `roca-js` (JS emission).

### Adding a new `Constraint` variant
1. Add the variant to `Constraint` enum in `roca-types/src/lib.rs`.
2. Update `convert.rs` to handle the new AST constraint.
3. Update `roca-cranelift` property test generation to fuzz the new constraint.
4. Update `roca-check` constraint validation rule.

### Adding a new keyword or built-in type
1. Add to `KEYWORDS` or `BUILTIN_TYPES` in `roca-ast/src/constants.rs`.
2. Update the tokenizer in `roca-parse` to recognize it.
3. Update LSP completions in `roca-lsp` if it should appear in autocomplete.
