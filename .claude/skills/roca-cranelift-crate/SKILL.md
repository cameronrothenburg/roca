---
name: roca-cranelift-crate
description: "Cranelift IR toolkit and memory ownership model for roca-cranelift. ALWAYS use this skill when reading, writing, reviewing, or modifying any file in crates/roca-cranelift/. This includes the Body/Function/Struct builder API, memory lifecycle (const=borrow, let=move, self=mutable-borrow, temporaries=immediate-free), emit helpers, the runtime registry, and memory tests."
---

# roca-cranelift -- Cranelift IR Toolkit

## Single Responsibility

Provides a high-level builder API (Function, Body, Struct, etc.) that maps language constructs to Cranelift IR -- control flow, variables, memory management, pattern matching -- without knowing anything about the source language's AST or orchestration.

## Boundaries

### Depends On

- **roca-types** -- `LangType` / `RocaType` for Roca-to-Cranelift type mapping
- **roca-runtime** -- re-exports `MEM`, `MemTracker`, `constraint_violated`; the `registry.rs` module wires all `extern "C"` host functions into the JIT via `runtime_funcs!` macro
- **cranelift-codegen, cranelift-frontend, cranelift-module, cranelift-jit** -- raw Cranelift IR generation (encapsulated behind the builder API)

### Depended On By

- **roca-native** -- the only consumer; calls the builder API to translate AST nodes into IR

### MUST NOT

- Import or reference `roca-ast`, `roca-parse`, `roca-check`, `roca-js`, or `roca-lsp` -- this crate is language-agnostic
- Know about Roca AST nodes, crash handlers, test runners, or property testing -- that is `roca-native`'s domain
- Implement stdlib functions -- that belongs in `roca-runtime`
- Decide which functions to compile or in what order -- orchestration belongs in `roca-native`

## Key Invariants

1. **Domain split** -- this crate owns **WHEN** memory is freed (the lifecycle). `roca-runtime` owns **HOW** values are freed (tags, layouts, deallocation). Body emits `call __free(ptr)`. Runtime does the rest.

2. **Builder API encapsulates all raw Cranelift IR** -- consumers (roca-native) call `Body`, `Function`, `Struct`, etc. They never touch `FunctionBuilder`, `ins.*`, or `cranelift_codegen` types directly.

3. **Three IR types only** -- `F64` (numbers), `I8` (booleans), `I64` (heap pointers). Cranelift doesn't need to know if an I64 is a string, array, or struct -- only that it's a heap pointer needing free.

4. **Runtime registry contract** -- `registry.rs` uses `runtime_funcs!` to declare every `extern "C"` function from `roca-runtime`. `register_symbols()` wires function pointers into the JIT linker. `declare_runtime()` tells Cranelift the signatures. `import_all()` makes them callable as `FuncRef`s keyed by `"__<key>"`. Signature mismatches cause silent runtime corruption.

5. **Temp tracking** -- Body maintains `temps: Vec<Value>` of I64 values not yet bound to a variable. `const_var`/`let_var` removes from temps (ownership transferred). At scope exit, remaining temps are freed alongside named variables.

## Ownership Model

Every heap value is just a pointer with a size. One owner. When the owner goes away, the value is freed. No reference counting. No type dispatch. No garbage collection.

### `const` -- immutable, scope-owned

A `const` binding owns a value until scope exit. Cannot be reassigned. When passed to a function, the function **borrows** the pointer -- caller retains ownership. Freed exactly once at scope exit.

```
const s = "hello"       → scope owns s
doSomething(s)           → borrows s, scope still owns it
return 42                → scope exit, free s
```

### `let` -- mutable, move semantics

A `let` binding owns a value. When passed to a function, **ownership moves** -- caller's slot is cleaned up, callee owns it. On reassignment, old value is freed before new one is stored.

```
let x = "hello"         → scope owns x
doSomething(x)           → ownership moves to doSomething
                         → scope cleans x's slot (x is gone)
let y = makeString()     → y now owns the returned value
y = "other"              → old y freed, new value owned
return y                 → ownership passes to caller
```

### `self` -- mutable borrow

A method receives `self` as a mutable borrow. Can read/write fields, but does not own the struct. Caller retains ownership. `self` is never freed by the method.

### Temporaries -- immediate cleanup

Any expression result not bound to `const`/`let` is a temporary. Freed at end of the statement that created it.

```
"a".split(",").join("-")
│         │         └─ result: bound or returned → has owner
│         └─ intermediate array: temporary → freed after join
└─ "," string arg: temporary → freed after split
```

### When to Free

| Event | What gets freed |
|-------|----------------|
| Scope exit (return) | All `const` and remaining `let` bindings |
| Reassignment (`let x = new`) | The old value in x's slot |
| Move to function (`fn(x)` where x is let) | x's slot in caller |
| Loop iteration end | Variables declared inside the loop body |
| Break / Continue | Loop-local variables before jumping |
| Statement end | All temporaries from that statement |

### How Body Tracks Ownership

Body maintains:
- `live_heap_vars: Vec<String>` -- names of heap variables in scope, in order of creation
- `loop_heap_base: usize` -- index into live_heap_vars marking where loop-local vars start

### Function Parameters

- **`const` parameter**: Caller retains ownership. Callee borrows. `is_heap = false` in callee's VarInfo.
- **`let` parameter**: Caller moves ownership. Caller cleans slot. `is_heap = true` in callee's VarInfo -- callee frees at scope exit.

## YAGNI Rules

- **No AST awareness** -- don't import roca-ast or pattern-match on AST nodes; that's roca-native's translation layer
- **No garbage collector or reference counting** -- single-owner model, period
- **No type dispatch in free** -- one `free(ptr)` call; runtime handles tag-based dispatch
- **No stdlib implementations** -- Body calls `__<key>` function refs; runtime provides them
- **No optimization passes** -- emit straightforward IR; let Cranelift optimize
- **No language-specific orchestration** -- don't decide compile order, don't walk source files

## Key Files

| File | Purpose |
|------|---------|
| `src/lib.rs` | Public API re-exports, module declarations, domain boundary docs |
| `src/api/body.rs` | `Body` struct -- variable binding, scope cleanup, control flow, memory lifecycle |
| `src/api/function.rs` | `Function`, `Method`, `Struct`, `Satisfies`, `RocaEnum`, `ExternFn`, `ExternContract` builders |
| `src/api/mod.rs` | API module re-exports, `ConstRef`/`MutRef`/`VarRef`/`Value` types |
| `src/context.rs` | `VarInfo`, `EmitCtx`, `CompiledFuncs`, `StructLayout`, `live_heap_vars` |
| `src/emit_helpers.rs` | `emit_scope_cleanup`, `emit_loop_body_cleanup` -- IR emission for free sequences |
| `src/registry.rs` | `runtime_funcs!` macro, `RuntimeFuncs`, `register_symbols`, `declare_runtime`, `import_all` |
| `src/builder/compiler.rs` | Low-level Cranelift `FunctionBuilder` wrapper |
| `src/builder/ir.rs` | IR instruction helpers (calls, loads, stores, branches) |
| `src/cranelift_type.rs` | `CraneliftType` -- Roca-to-IR type mapping |
| `src/lang_type.rs` | `LangType` -- language-level type abstraction |
| `src/module.rs` | `JitModule`, `FnDecl`, `declare_functions` -- module-level function management |
| `src/helpers.rs` | Shared utility functions |
| `src/types.rs` | Internal type definitions |
| `src/tests_memory.rs` | Memory lifecycle tests (allocs == frees invariant) |

## Test Patterns

Tests live in `src/tests_memory.rs` using the `mem_test!` macro:

```rust
mem_test!(test_name, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s = body.string("hello");
        body.const_var("s", s);
        let r = body.number(42.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 42.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "leak: {} allocs, {} frees", allocs, frees);
});
```

Rules:
- `MEM.reset()` before calling the JIT function, not before compile
- Assert `allocs == frees` -- the zero-leak invariant
- Every test should verify one ownership scenario (scope exit, reassignment, move, temporary)
- End-to-end Roca compilation tests belong in `roca-native`, not here
