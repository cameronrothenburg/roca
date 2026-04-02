---
name: roca-cranelift-crate
description: "Memory ownership model for roca-cranelift — the rules that govern every operation in the crate. ALWAYS use this skill when reading, writing, reviewing, or modifying any file in crates/roca-cranelift/ or crates/roca-native/src/emit/. This includes Body API changes, function compilation, control flow, variable binding, scope cleanup, tests, and any new features. The memory model (const=borrow, let=move, self=mutable-borrow, temporaries=immediate-free) is the foundation — every code change must respect it."
---

# Cranelift Memory Model

Every heap value is just a pointer with a size. One owner. When the owner goes away, the value is freed. No reference counting. No type dispatch. No garbage collection.

## Ownership Rules

### `const` — immutable, scope-owned

A `const` binding allocates a value and owns it until the scope exits. It cannot be reassigned. When passed to a function, the function **borrows** the pointer — the caller retains ownership. The value is freed exactly once: at scope exit.

```
const s = "hello"       → scope owns s
doSomething(s)           → borrows s, scope still owns it
return 42                → scope exit, free s
```

### `let` — mutable, move semantics

A `let` binding owns a value. When the value is passed to a function, **ownership moves** — the caller's slot is cleaned up and the function now owns the value. If the function returns it, ownership passes to whoever binds the return value.

```
let x = "hello"         → scope owns x
doSomething(x)           → ownership moves to doSomething
                         → scope cleans x's slot (x is gone)
                         → doSomething owns it now

let y = makeString()     → makeString created it, returned it
                         → y now owns it
y = "other"              → old y freed, new value owned
return y                 → ownership passes to caller
```

On reassignment, the old value is freed before the new one is stored.

### `self` — mutable borrow

A method receives `self` as a mutable borrow. It can read and write fields (`self.x = 5`), but it does not own the struct. The caller retains ownership. `self` is never freed by the method.

### Temporaries — immediate cleanup

Any expression result that is not bound to a `const` or `let` is a temporary. Temporaries have no owner and must be freed at the end of the statement that created them.

```
"a".split(",").join("-")
│         │         └─ result: bound to variable or returned → has owner
│         └─ intermediate array: temporary → freed after join completes
└─ "," string arg: temporary → freed after split completes
```

## The IR Model

At the IR level, there are only three types:

| IR Type | What | Heap? |
|---------|------|-------|
| `F64` | Numbers | No |
| `I8` | Booleans | No |
| `I64` | Heap pointer | Yes — needs free |

Cranelift does not need to know if an I64 is a string, array, or struct. It only needs to know: **this is a heap pointer, and here is when to free it.**

## Cleanup: One Function

The runtime provides a single `free(ptr)` for all heap values. The allocator tags each allocation so `free` knows the layout. Cranelift emits one call: `free(ptr)`. No strategy dispatch, no type registry, no per-type free functions.

This means:
- No `CleanupStrategy` enum with 7 variants
- No `CleanupRegistry` with override maps
- No `value_cleanups` HashMap tracking what Body method created what
- No `_typed` variants on const_var/let_var
- Body just tracks: "this slot has a heap pointer" → emit free at the right time

## When to Free

| Event | What gets freed |
|-------|----------------|
| Scope exit (return) | All `const` and remaining `let` bindings |
| Reassignment (`let x = new`) | The old value in x's slot |
| Move to function (`fn(x)` where x is let) | x's slot in caller |
| Loop iteration end | Variables declared inside the loop body |
| Break / Continue | Loop-local variables before jumping |
| Statement end | All temporaries from that statement |

## How Body Tracks Ownership

Body maintains:
- `live_heap_vars: Vec<String>` — names of heap variables in scope, in order of creation
- `loop_heap_base: usize` — index into live_heap_vars marking where loop-local vars start

When a variable is created:
1. Allocate a stack slot for the pointer
2. Mark `is_heap = true` if the value is I64
3. Add the name to `live_heap_vars`

When scope exits:
1. Iterate `live_heap_vars`
2. For each heap var, emit `free(ptr)` 
3. Then emit the return

When a loop iteration ends:
1. Iterate `live_heap_vars` from `loop_heap_base` onward
2. Free those (loop-local) vars
3. Jump back to header

## Function Parameters

Parameters follow the ownership rules:

- **`const` parameter**: Caller retains ownership. Callee borrows the pointer. Callee does NOT free it. The parameter's `is_heap` is `false` in the callee's VarInfo — it's borrowed.

- **`let` parameter**: Caller moves ownership. Caller cleans its slot. Callee now owns it. The parameter's `is_heap` is `true` in the callee's VarInfo — callee frees at its scope exit.

## Writing Memory Tests

Pattern for cranelift-level tests in `crates/roca-cranelift/src/tests_memory.rs`:

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
- Assert `allocs == frees` — the zero-leak invariant
- Every test should verify one ownership scenario (scope exit, reassignment, move, temporary)

## Key Files

| File | Purpose |
|------|---------|
| `crates/roca-cranelift/src/api/body.rs` | Body struct, variable binding, scope cleanup |
| `crates/roca-cranelift/src/context.rs` | VarInfo, EmitCtx, live_heap_vars |
| `crates/roca-cranelift/src/emit_helpers.rs` | emit_scope_cleanup, emit_loop_body_cleanup |
| `crates/roca-cranelift/src/tests_memory.rs` | Generic memory tests |
| `crates/roca-runtime/src/lib.rs` | MEM tracker, allocator, free functions |
