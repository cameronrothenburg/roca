---
name: stdlib-module
description: How to add a new stdlib module to Roca. Follow this when creating std::* modules with contracts, JS wrappers, runtime bridges, and tests.
---

# Adding a Stdlib Module

## Before you start

1. Check existing types in `packages/stdlib/primitives.roca` — don't duplicate what's there (String, Number, Bool, Array, Bytes, Buffer, Optional, Map, Loggable, Serializable, Deserializable)
2. Check existing modules in `packages/stdlib/*.roca` — json, encoding, http, crypto, url, time
3. Check MDN for every property and return value — map `null | undefined` to `Optional<Type>`, never-null to concrete types
4. Prefer enums over raw strings for fixed value sets
5. Prefer struct field constraints (`min`, `max`, `contains`, `pattern`, `default`) over runtime validation
6. `from` is a keyword — don't use it as a method name

## File structure

Every stdlib module needs up to 4 files:

```
packages/stdlib/{name}.roca      # Contract (always required)
packages/stdlib/{name}.js        # JS wrapper (always required)
packages/runtime/{name}-bridge.js # Runtime bridge (only if bare V8 lacks the API)
tests/verify/{name}.rs           # Verify tests (always required)
```

If a Rust op is needed, add it to `src/cli/runtime.rs`.

## 1. Contract file — `packages/stdlib/{name}.roca`

```roca
/**
 * [Description of the module]
 * Import with: import { ContractName } from std::[name]
 *
 * Compiled output uses globalThis.X — works in browsers, Node, Bun, Workers.
 */
pub extern contract ContractName {
    /// [Doc comment for method]
    methodName(param: Type) -> ReturnType

    /// [Method that can fail]
    riskyMethod(param: Type) -> ReturnType, err {
        err error_name = "human readable message"
        err another_error = "another message"
    }

    /// [Nullable return — use Optional]
    lookup(key: String) -> Optional<String>

}
```

Rules:
- Always `pub extern contract`
- Doc comment on every method (`///`)
- Error names are `lowercase_snake_case`
- No mock blocks needed — compiler auto-generates stubs from return types

## 2. JS wrapper — `packages/stdlib/{name}.js`

```js
const ContractName = (() => {
    const _nativeApi = globalThis.NativeApi;

    return {
        methodName(param) {
            return _nativeApi.method(param);
        },
        riskyMethod(param) {
            try { return { value: _nativeApi.risky(param), err: null }; }
            catch (e) { return { value: null, err: { name: "error_name", message: e.message } }; }
        },
        lookup(key) {
            return _nativeApi.get(key); // returns null if missing — maps to Optional
        },
    };
})();
```

Rules:
- IIFE pattern: `const Name = (() => { ... })();`
- Use `globalThis.X` for Web APIs — **never** `Deno.core`
- Error-returning methods return `{ value, err }` protocol
- Non-error methods return values directly
- Deduplicate shared logic into helper functions
- The compiled output must work in any JS environment

## 3. Native runtime — `src/native/runtime/stdlib.rs`

Each stdlib method needs a native (Cranelift) implementation for `roca build --native` and the REPL.

```rust
// In src/native/runtime/stdlib.rs
pub extern "C" fn roca_contract_method(param: i64) -> i64 {
    let input = read_cstr(param);
    alloc_str(&result)
}
```

Then register in `src/native/runtime/mod.rs` inside `runtime_funcs!`:
```
(contract_method, "roca_contract_method", roca_contract_method, [types::I64], [types::I64]),
```

Rules:
- Use `read_cstr(ptr)` to read string args, `alloc_str(&s)` to return strings
- Numbers are `f64`, bools are `u8`, strings/arrays/structs are `i64` pointers
- Error-returning methods: return `(value, err_tag)` where `err_tag: u8` (0 = no error)
- Add null guards: `if ptr == 0 { return 0; }`
- Track memory: `MEM.track_alloc(size)` / `MEM.track_free(size)` for heap allocations
- Add a free function if the type is heap-allocated (like Map)
- For extern contracts: auto-stubs are generated from types — no mock blocks needed

## 4. Runtime bridge (V8 only) — `packages/runtime/{name}-bridge.js`

Only needed if bare V8 doesn't have the API (e.g. URL, crypto).

```js
/**
 * [Name] bridge for bare V8.
 * Backed by Rust op (op_name) via deno_core.
 */
if (typeof NativeApi === "undefined") {
    globalThis.NativeApi = {
        method(param) {
            return Deno.core.ops.op_name(param);
        },
    };
}
```

Rules:
- Guarded with `if (typeof X === "undefined")`
- References `Deno.core.ops` — this is fine, bridges only run in embedded runtime
- Load in `src/cli/runtime.rs` bootstrap via `include_str!`
- **Never** referenced in compiled output

## 5. Rust ops (V8 only) — `src/cli/runtime.rs`

```rust
#[op2]
#[string]
fn op_name(#[string] input: &str) -> String {
    // Use proper Rust crates, never raw format! for JSON
    serde_json::json!({ "field": value }).to_string()
}

extension!(
    roca_runtime,
    ops = [op_capture_log, op_url_parse, op_sha256, op_sha512, op_random_uuid, op_name],
);
```

Rules:
- Use `#[op2]` macro
- Register in `extension!` ops list
- Return simple types (String, bool)
- Use `serde_json` for structured returns — never raw `format!` interpolation (injection risk)
- Load bridge in `bootstrap()` function

## 6. Tests — `tests/verify/{name}.rs`

```rust
use super::harness::run;

#[test]
fn contract_name_method() {
    assert_eq!(run(
        r#"
        import { ContractName } from std::[name]
        /// Does the thing
        pub fn do_thing(input: String) -> String {
            const result = ContractName.method(input)
            return result
            crash { ContractName.method -> fallback("default") }
            test { self("input") == "expected" }
        }
        "#,
        r#"console.log(do_thing("input"));"#,
    ), "expected");
}
```

Rules:
- Harness runs `check()` before `emit()` — code must pass ALL checker rules
- Only `missing-doc` and `missing-test` are filtered
- No `if false { return err.x }` hacks — ever
- No `let val, err = call()` — use crash blocks
- Standalone functions use `fallback` or `skip` for error handling
- To propagate errors, use a struct with contract block:
  ```roca
  pub struct Service {
      call(input: String) -> String, err {
          err failed = "something failed"
      }
  }{
      pub fn call(input: String) -> String, err {
          // ...
          crash { dependency -> halt }
          test { self("ok") is Ok  self("bad") is err.failed }
      }
  }
  ```
- Register in `tests/verify/main.rs`

## 7. Error rules

- **Only struct contract blocks and extern contracts define errors** — standalone functions cannot mint new error names (`no-fn-error-def` rule)
- `halt` propagates callee errors automatically — those errors are auto-declared on the function
- Every declared error must be tested in the test block
- Every error-returning call needs a crash entry

## 8. Documentation

After adding the module:
- Update `docs/src/integration/stdlib-modules.md` — add method table + usage example
- Update `docs/src/reference/stdlib.md` — if new contracts added to primitives
- Update `docs/src/reference/compiler-rules.md` — if new checker rules added

## 9. Verification checklist

```bash
cargo test                    # all tests pass, zero warnings
cargo install --path .        # install latest
cd examples/worker && roca build  # worker example still works
```

- No Deno references in compiled `.js` output
- Run `/simplify` — no copy-paste, no injection bugs, no unnecessary allocations
- All verify tests pass the checker (no skipped rules beyond doc/test)
