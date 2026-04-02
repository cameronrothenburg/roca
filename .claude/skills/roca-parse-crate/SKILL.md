---
name: roca-parse-crate
description: "Parser and module resolver for roca-parse and roca-resolve. ALWAYS use this skill when reading, writing, reviewing, or modifying any file in crates/roca-parse/ or crates/roca-resolve/. This includes the tokenizer, recursive descent parser, all parse sub-modules (expr, function, struct, contract, crash, test_block, satisfies, string_interp), module resolution, and the contract registry."
---

# roca-parse + roca-resolve -- Parsing & Module Resolution

## Single Responsibility
- **roca-parse**: Tokenize `.roca` source into a `Token` stream, then parse it into a `roca_ast::SourceFile` via recursive descent. No validation, no semantics -- pure syntax.
- **roca-resolve**: Walk `import` statements to load neighbouring `.roca` files, resolve stdlib modules, and build a `ContractRegistry` that aggregates contracts, structs, enums, and satisfies relationships across every file in a project.

## Boundaries

### Depends On
| Crate | Used By |
|---|---|
| `roca-ast` | Both crates (AST node types, `SourceFile`, `Item`, etc.) |
| `roca-errors` | roca-parse only (`ParseError` type) |
| `roca-parse` | roca-resolve (calls `parse()` / `try_parse()` to load imported files) |

### Depended On By
| Consumer | Uses |
|---|---|
| `roca-check` | Both (parses source, resolves imports + registry for checker rules) |
| `roca-cli` | Both (build/check commands parse then resolve entire projects) |
| `roca-native` | roca-parse (parses source for Cranelift JIT compilation) |
| `roca-js` | roca-parse (parses source for JS emission) |
| `roca-lsp` | Both (parse + resolve for diagnostics and completions) |
| root `src/` | Both (re-exported via workspace for the binary) |

### MUST NOT
- **No semantic validation** -- the parser must not check types, enforce contracts, or validate that functions have test blocks. That is `roca-check`'s job.
- **No code generation** -- no JS emission, no Cranelift IR. Parser produces AST and stops.
- **No runtime behavior** -- no function execution, no stdlib runtime stubs.
- **No error recovery in parser** -- `parse()` panics on syntax errors; `try_parse()` returns `Result`. Neither attempts partial recovery or error correction.
- **Registry must not type-check** -- `ContractRegistry` collects declarations but never validates satisfies implementations or method signatures. The checker does that.

## Key Invariants

### Tokenizer
- `tokenize(source: &str) -> Vec<Token>` produces a flat token stream ending with `Token::EOF`.
- Every keyword has its own `Token` variant (`Fn`, `Struct`, `Contract`, `Crash`, `Test`, `Match`, etc.). Identifiers are `Token::Ident(String)`.
- String literals with `${}` interpolation are handled by the `string_interp` module during expression parsing, not during tokenization.

### Parser Architecture
- **Single `Parser` struct** defined in `expr.rs` (not `parser.rs`). Holds `tokens: Vec<Token>` and `pos: usize`. All parse methods are `impl Parser` blocks spread across sub-modules.
- **Recursive descent** -- each construct has its own file with parse methods:
  - `parser.rs` -- `parse_file()` (top-level dispatch), `parse_import()`, `parse_enum()`, `parse_extern_fn()`
  - `expr.rs` -- `parse_expr()`, binary ops, literals, calls, match, field access
  - `function.rs` -- `parse_function()`, params, return types, body statements
  - `struct_def.rs` -- `parse_struct_def()`, fields, signatures, struct body methods
  - `contract.rs` -- `parse_contract()`, contract functions/fields
  - `satisfies.rs` -- `parse_satisfies()`, satisfies block methods
  - `crash.rs` -- `parse_crash()`, crash strategies (halt, retry, fallback, skip, panic)
  - `test_block.rs` -- `parse_test_block()`, inline test assertions
  - `string_interp.rs` -- interpolated string parsing (`"hello ${name}"`)
- **Entry points**: `parse(src) -> SourceFile` (panics) and `try_parse(src) -> Result<SourceFile, ParseError>` (fallible, used by LSP).
- `ParseResult<T>` is `Result<T, ParseError>`.

### Resolver Architecture
- `find_imported_fn(name, file, source_dir) -> Option<ResolvedFn>` -- follows a single function name through imports to find its signature. Used by checker rules.
- `resolve_file(path) -> ResolvedProject` -- recursively parses a file and all its imports, builds combined registry.
- `resolve_directory(dir) -> ResolvedProject` -- finds all `.roca` files in a directory, parses them, follows imports, builds combined registry.
- `collect_file()` tracks already-parsed paths in a `HashMap<PathBuf, SourceFile>` to avoid cycles.
- Import path resolution searches: `from_dir`, `from_dir/src`, `.`, `src`.

### Contract Registry
- `ContractRegistry::build(file)` loads stdlib (cached via `LazyLock`) then the given file.
- Stdlib source is `include_str!` concatenation of `packages/stdlib/core/*.roca`, parsed once per process.
- Dynamic stdlib modules loaded from `packages/stdlib/{subdir}/{name}.roca` at runtime.
- Three maps: `contracts` (contract + extern contract), `struct_contracts` (struct declarations), `satisfies_map` (type -> list of contracts it satisfies).
- `type_accepts(expected, actual)` checks identity or satisfies relationship.

## YAGNI Rules
- Do not add error recovery or partial parsing -- the LSP uses `try_parse()` which returns an error; it does not need a partial AST.
- Do not add incremental parsing -- full re-parse is fast enough for the current scale.
- Do not add semantic validation in the parser -- no type checking, no "function must have test block" enforcement, no "crash strategy must be valid" checks. Parser accepts syntactically valid constructs only.
- Do not cache parsed imports in the resolver -- `collect_file()` already deduplicates by path within a single resolution pass.
- Do not add glob or wildcard imports -- Roca imports are explicit named imports only.

## Key Files

### roca-parse (`crates/roca-parse/`)
| File | Purpose |
|---|---|
| `src/lib.rs` | Public API: re-exports `parse()`, `try_parse()`, `tokenize()` |
| `src/tokenizer.rs` | Lexer: `Token` enum + `tokenize()` function |
| `src/expr.rs` | `Parser` struct + expression parsing (binops, calls, match, literals) |
| `src/parser.rs` | `parse_file()` top-level dispatch, imports, enums, extern fns |
| `src/function.rs` | Function parsing: params, return types, body, statements |
| `src/struct_def.rs` | Struct definition: fields, signatures, body methods |
| `src/contract.rs` | Contract definition: function signatures, fields |
| `src/satisfies.rs` | Satisfies blocks: struct-contract implementation |
| `src/crash.rs` | Crash block parsing: strategies |
| `src/test_block.rs` | Inline test block parsing |
| `src/string_interp.rs` | String interpolation: `${}` expansion |
| `src/parser_tests.rs` | Integration tests: full source -> AST round-trips |
| `src/parser_tests_extra.rs` | Additional parser integration tests |
| `src/expr_tests.rs` | Unit tests: expression parsing in isolation |

### roca-resolve (`crates/roca-resolve/`)
| File | Purpose |
|---|---|
| `src/lib.rs` | Public API: re-exports `ContractRegistry`, `find_imported_fn`, `ResolvedFn`, `resolve_file`, `resolve_directory` |
| `src/resolve.rs` | Import resolution: `find_imported_fn()`, `resolve_file()`, `resolve_directory()`, recursive `collect_file()` |
| `src/registry.rs` | `ContractRegistry`: builds from AST, loads stdlib, `type_accepts()`/`type_satisfies()` queries |

## Test Patterns

### parser_tests.rs -- Full round-trip tests
Parse a complete Roca source string, assert on `file.items.len()` and destructure items to verify structure:
```rust
let file = parse(src);
assert_eq!(file.items.len(), 3);
assert!(matches!(file.items[0], Item::Contract(_)));
```

### expr_tests.rs -- Isolated expression tests
Create a `Parser` directly from tokenized input, call `parse_expr()`, match on the result:
```rust
let mut p = Parser::new(tokenize("1 + 2"));
let expr = p.parse_expr().unwrap();
assert!(matches!(expr, Expr::BinOp { op: BinOp::Add, .. }));
```

### Adding new syntax
1. Add token variant to `Token` enum in `tokenizer.rs` if needed.
2. Add AST node to `roca-ast`.
3. Add parse method on `Parser` in the appropriate sub-module.
4. Wire it into `parse_file()` dispatch in `parser.rs`.
5. Add a round-trip test in `parser_tests.rs` and/or an expression test in `expr_tests.rs`.
