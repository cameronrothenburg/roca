---
name: roca-runtime-crate
description: "Host runtime and stdlib for roca-runtime. ALWAYS use this skill when reading, writing, reviewing, or modifying any file in crates/roca-runtime/. This includes the memory allocator, MEM tracker, stdlib functions (string ops, array ops, struct ops), and all extern C function signatures that Cranelift JIT calls."
---

# roca-runtime -- Host Runtime & Stdlib

## Single Responsibility

Provides the host-side `extern "C"` functions that JIT-compiled Roca code calls at runtime: memory allocation/tagging/freeing, stdlib operations (strings, arrays, maps, math, path, process, crypto, URL, JSON, HTTP, file I/O, encoding, timing), and the thread-local `MemTracker` for leak detection.

## Boundaries

### Depends On

Leaf crate -- no internal Roca crate dependencies. External deps: `uuid`, `url`, `serde_json`, `sha2`, `base64`, `reqwest`, `tokio`.

### Depended On By

- **roca-cranelift** -- `registry.rs` does `pub use roca_runtime::*` and wires every `extern "C"` function into the JIT via `runtime_funcs!` macro + `JITBuilder::symbol()`.
- **roca-native** -- re-exports through roca-cranelift for test runner orchestration.

### MUST NOT

- Import any compiler crate (roca-cranelift, roca-native, or anything from `src/`).
- Know about Cranelift IR, AST nodes, type checking, or codegen.
- Own **when** values are freed -- that is roca-cranelift's `Body` responsibility. This crate only provides `roca_free(ptr)`.
- Own test orchestration -- that belongs to roca-native's test runner.

## Key Invariants

1. **All public functions are `extern "C"`** -- Cranelift calls them by symbol name via the C ABI. No Rust-only calling conventions.
2. **Allocation tagging is mandatory** -- every heap allocation must call `tag_alloc(ptr, TAG_*, size)` and `MEM.track_alloc(size)`. Without the tag, `roca_free` silently ignores the pointer (leak).
3. **`roca_free` is the single free path** -- reads the tag from `ALLOC_TAGS`, dispatches by type (TAG_STRING=1, TAG_VEC=2, TAG_MAP=3, TAG_BOX=4), deallocates with correct layout, recursively frees tracked children.
4. **TAG_VEC covers arrays, structs, and enums** -- all are `Vec<i64>` field slots. `roca_struct_alloc`, `roca_array_new`, and enum allocs all use TAG_VEC.
5. **TAG_BOX layout: `[drop_fn: u64][total_size: u64][payload...]`** -- `roca_free` reads the 16-byte header behind the payload pointer, calls the drop trampoline if non-null, then deallocates the full block. Constants: `BOX_HEADER=16`, `BOX_ALIGN=16`.
6. **MEM tracker is thread-local** -- each test thread has independent alloc/free/retain/release/live_bytes counters. No cross-thread races.
7. **Signature changes break the JIT** -- every `extern "C"` function signature here must exactly match the `[param_types]` and `[return_types]` declared in `roca-cranelift/src/registry.rs`'s `runtime_funcs!` macro.
8. **Strings are null-terminated C strings** allocated via `alloc_str` (raw `std::alloc::alloc` + TAG_STRING). `read_cstr` reads them back via `CStr::from_ptr`.
9. **Error tuple functions return `(i64, u8)`** -- pointer + error tag. Tag 0 = OK. These use `#[allow(improper_ctypes_definitions)]` because C ABI multi-return is non-standard but Cranelift handles it via multi-value returns.

## YAGNI Rules

- **No garbage collector** -- single-owner model, freed at scope exit by cranelift.
- **No reference counting** -- no Rc, Arc, or refcount fields. One owner, one free.
- **No type dispatch in free beyond the 4 tags** -- TAG_STRING, TAG_VEC, TAG_MAP, TAG_BOX cover everything. New heap types must fit one of these or get a new tag (rare).
- **No async runtime exposed to Roca** -- `tokio_rt()` is internal to `roca_wait_all`/`roca_wait_first`/HTTP. Roca code does not see async/await.
- **No global mutable state beyond `ALLOC_TAGS` and `MEM`** -- keep the threading model simple.

## Key Files

| File | Purpose |
|------|---------|
| `src/lib.rs` | Allocation tags (`ALLOC_TAGS`), `tag_alloc`/`untag_alloc`, `alloc_str`/`read_cstr`, `MemTracker`/`MEM`, `roca_free`, struct ops, `roca_string_new`, `roca_f64_to_bool`, constraint validation, BOX_HEADER/BOX_ALIGN constants |
| `src/stdlib.rs` | All stdlib `extern "C"` functions: I/O (`roca_print*`), string ops, map ops, array ops, math, path, process, timing/async (`roca_sleep`, `roca_wait_all`, `roca_wait_first`), file I/O (`roca_fs_*`), crypto, URL, encoding, JSON, HTTP |

## ABI Contract

### How Cranelift Registers These Functions

In `roca-cranelift/src/registry.rs`:
1. `runtime_funcs!` macro declares a `RuntimeFuncs` struct with one `FuncId` per function.
2. `register_symbols()` calls `JITBuilder::symbol(name, fn_ptr)` for each function -- this is how the JIT linker resolves symbols at runtime.
3. `declare_runtime()` calls `declare_fn(module, symbol, params, returns)` for each -- this tells Cranelift the signature so it generates correct call instructions.
4. `import_all()` makes each function callable within a Cranelift function body as a `FuncRef`, keyed by `"__<key>"` (e.g., `"__string_concat"`).

### What Happens If a Signature Changes

If you change a function's parameter types, return types, or parameter count in `roca-runtime` without updating the corresponding `runtime_funcs!` entry in `registry.rs`, Cranelift will generate calls with the wrong ABI. This causes:
- Incorrect register usage (wrong types passed)
- Stack corruption (wrong number of arguments)
- Silent data corruption or segfaults at JIT execution time
- No compile-time error -- the mismatch is only visible at runtime

### How to Safely Add a New Stdlib Function

1. Add the `pub extern "C" fn roca_<name>(...)` in `stdlib.rs` (or `lib.rs` for core ops).
2. Ensure it calls `tag_alloc` + `MEM.track_alloc` for any heap allocation.
3. Add the matching entry in `roca-cranelift/src/registry.rs`'s `runtime_funcs!` macro with exact param/return types.
4. If it maps to a Roca stdlib method (e.g., `String.foo`), add the contract alias mapping in `stdlib_key_to_contract` in registry.rs.
5. Use the function via its `__<key>` name in cranelift codegen.

## Test Patterns

- **Runtime functions are tested through roca-cranelift and roca-native**, not directly. The memory lifecycle tests in `roca-cranelift/src/tests_memory.rs` compile Roca snippets to JIT, execute them, then assert `MEM.stats()` shows allocs == frees and live_bytes == 0.
- **`MEM.reset()` before each test, `MEM.assert_clean()` after** -- this is the standard pattern for verifying no leaks.
- **`MEM.set_debug(true)`** enables `[mem]` trace logging to stderr for debugging allocation issues.
- Unit tests for individual stdlib functions (pure logic, not memory lifecycle) can live in this crate, but most coverage comes from integration tests that exercise the full JIT pipeline.
