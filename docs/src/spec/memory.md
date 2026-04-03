# 5. Memory Model

This section defines how Roca manages memory across its two compilation targets. The same source produces both JavaScript output and native machine code via Cranelift. The memory model is the bridge — ownership rules enforced at compile time translate to physical operations on native and correctness guarantees on JS.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

---

## 5.1 Ownership Rules

Every heap value in Roca has exactly one owner. Ownership is determined by `const` (owned) and `let` (borrowed), and function parameters declare their intent with `o` (owned/consuming) and `b` (borrowed).

### Rule 1: `const` is always an owner

A `const` binding creates a value and owns it. The `const` is responsible for the value's lifetime. It is freed at its last use.

```roca
const file = open("data.txt")
const count = 42
const user = User { name: "Alice", age: 30 }
```

A conforming compiler MUST reject any value creation without a `const` binding.

**Error: `E-OWN-001` — value must be owned by a const**

### Rule 2: `let` is always a borrow, always derived from a `const`

A `let` binding MUST derive from an existing `const`. It is never a source of data — it is always a read-only reference to something a `const` owns.

```roca
const user = User { name: "Alice", age: 30 }
let name = user.name
let age = user.age
```

A conforming compiler MUST reject `let` bindings that create new values:

```roca
let user = User { name: "Alice", age: 30 }  // ERROR
let result = compute()                        // ERROR
```

**Error: `E-OWN-002` — let must derive from an existing const**

### Rule 3: You MUST `let` before passing to a borrowing (`b`) parameter

A `const` MUST NOT be passed directly to a function parameter declared with `b`. The borrow MUST be named explicitly with a `let` binding first.

```roca
const file = open("data.txt")
let borrowed = file
process(borrowed)         // process(b file) borrows it
// file still valid here
```

A conforming compiler MUST reject direct `const` usage in `b` parameter positions:

```roca
process(file)             // ERROR: must let before passing to b parameter
```

**Error: `E-OWN-003` — const cannot be passed directly to a borrowing parameter; use let**

### Rule 4: Passing a `const` directly to an `o` parameter is a move

When a `const` is passed directly to a function parameter declared with `o`, ownership transfers. The caller's `const` is consumed and MUST NOT be used after the call.

```roca
const file = open("data.txt")
consume(file)             // consume(o file) takes ownership
// file is dead — any use is a compile error
```

**Error: `E-OWN-004` — use after move; value was consumed by [function] at line [N]**

### Rule 5: Function parameters declare intent with `o` and `b`

Every function parameter MUST declare one of:

- `b` — **borrowed**. The caller retains ownership. The function reads but does not consume.
- `o` — **owned**. The caller transfers ownership. The function is responsible for the value.

```roca
pub fn process(b file, b config) -> Result    // borrows both
pub fn consume(o file) -> Ok                  // takes ownership of file
pub fn transform(o data) -> Data              // takes ownership, returns new owned value
```

A conforming compiler MUST reject parameters without `o` or `b`:

```roca
pub fn process(file) -> Ok    // ERROR: parameter must declare o or b
```

**Error: `E-OWN-005` — parameter must declare ownership intent (o or b)**

### Rule 6: Return values are always owned (`const`)

A function MUST return an owned value. The caller always receives ownership. Returning a borrow is illegal.

```roca
pub fn get_name(b user) -> String {
    const name = user.name.copy()  // must copy because user is borrowed
    return name                     // caller receives owned String
}

const user = User { name: "Alice" }
let borrowed = user
const name = get_name(borrowed)    // name is owned by caller
```

A conforming compiler MUST reject functions that return borrowed values:

```roca
pub fn get_ref(b user) -> b String    // ERROR: cannot return a borrow
```

**Error: `E-OWN-006` — cannot return a borrowed value; return an owned copy**

### Rule 7: Containers always copy borrowed values

When a borrowed value (`let`) is inserted into a container (struct field, array, map), the compiler MUST insert a copy. The container always owns its elements.

A conforming compiler SHOULD emit a structured note when an implicit copy is inserted:

```roca
const user = User { name: "Alice" }
let name = user.name
const names = []
names.push(name)          // compiler copies name into names
                          // NOTE[E-OWN-007]: implicit copy at line N
```

**Note: `E-OWN-007` — implicit copy: borrowed value copied into container**

---

## 5.2 Second-Class References

References in Roca are **second-class**. They exist only as `let` bindings derived from a `const`. They MUST NOT be stored in struct fields or returned from functions.

### 5.2.1 What Is Allowed

```roca
const user = User { name: "Alice", age: 30 }
let name = user.name          // borrow a field
let borrowed = user           // borrow the whole struct
process(borrowed)             // pass borrow to b parameter
```

### 5.2.2 What Is Illegal

```roca
pub struct Cache {
    data: let String           // ERROR: cannot store a borrow in a struct
}

pub fn get_ref(b user) -> b String {
    return user.name           // ERROR: cannot return a borrow
}
```

**Error: `E-OWN-008` — references are second-class; cannot be stored in struct fields or returned**

### 5.2.3 Why

Storing references in data structures is the source of most complexity in ownership systems — lifetime annotations, variance analysis, `Pin<T>`, NLL, Polonius. By banning stored references, Roca eliminates this entire complexity class. The tradeoff is that structs must copy or take ownership of field values, which is natural for a language targeting JS.

---

## 5.3 Last-Use Destruction

Owned values (`const`) are freed at their **last use**, not at scope exit.

### 5.3.1 How It Works

```roca
pub fn process(b name) -> String {
    const upper = name.toUpperCase()    // name's last use was the line above
    const greeting = "Hello, " + upper  // upper consumed here
    return greeting                     // moved to caller — not freed
}
```

The compiler determines the last use of each `const` via static analysis. Destruction is inserted immediately after.

### 5.3.2 Control Flow

Both branches of an `if` MUST consume the same set of owned values:

```roca
// VALID — data consumed in both branches
if condition {
    send(data)
} else {
    log(data)
}

// INVALID — asymmetric consumption
if condition {
    send(data)      // consumes data
} else {
    // data not consumed — ERROR
}
```

**Error: `E-OWN-009` — owned value consumed in one branch but not the other**

Loops MUST NOT consume owned values from outer scopes unless reassigned within the loop body:

```roca
const data = load()
while running {
    process(data)       // ERROR: data consumed in loop without reassignment
}
```

**Error: `E-OWN-010` — owned value consumed in loop without reassignment**

---

## 5.4 Dual-Target Semantics

The source-level rules are identical for both targets. The backends translate differently.

### 5.4.1 Native (Cranelift)

| Operation | What happens |
|-----------|-------------|
| `const x = ...` | `mem_struct_new` / `mem_string_new` — physical allocation |
| Last use of `const` | `mem_free` — physical deallocation |
| `o` parameter | Ownership transfers, caller emits no free |
| `b` parameter | Pointer passed, callee does not free |
| Struct field store | `mem_struct_set_owned` — move if owned, copy if borrowed |
| Error path | Cleanup frees all live `const` values |

### 5.4.2 JavaScript

| Operation | What happens |
|-----------|-------------|
| `const x = ...` | `new Object()` — GC managed |
| Last use of `const` | Nothing — GC collects when unreachable |
| `o` parameter | Compile error on use-after-move (same as native) |
| `b` parameter | Reference passed (JS semantics) |
| Unique ownership proven | `obj.field = x` (direct mutation, not spread) |
| Resource cleanup | `using` declaration with `Symbol.dispose` |

---

## 5.5 Error Codes

| Code | Rule | Condition |
|------|------|-----------|
| `E-OWN-001` | 1 | Value created without a const owner |
| `E-OWN-002` | 2 | Let binding creates a new value instead of borrowing |
| `E-OWN-003` | 3 | Const passed directly to a `b` parameter |
| `E-OWN-004` | 4 | Use after move — value already consumed |
| `E-OWN-005` | 5 | Parameter missing `o` or `b` declaration |
| `E-OWN-006` | 6 | Function returns a borrowed value |
| `E-OWN-007` | 7 | Implicit copy into container (note, not error) |
| `E-OWN-008` | 2nd-class | Reference stored in struct field or returned |
| `E-OWN-009` | Control flow | Value consumed in one branch but not the other |
| `E-OWN-010` | Loops | Value consumed in loop without reassignment |

Error messages are generated by the feedback crate — see [AI Feedback Loop](./feedback.md).

---

## 5.6 Runtime Memory API (roca-mem)

All physical memory operations on the native path go through a single crate: `roca-mem`.

### 5.6.1 Allocation

| Function | Purpose |
|----------|---------|
| `mem_string_new(src)` | Copy a C string into a tracked allocation |
| `mem_struct_new(num_fields, type_id)` | Allocate a zero-initialized struct with type identity |
| `mem_array_new()` | Allocate an empty array |
| `mem_map_new()` | Allocate an empty map |

### 5.6.2 Struct Field Access

| Function | Purpose |
|----------|---------|
| `mem_struct_get_f64(ptr, idx)` | Read a number field |
| `mem_struct_set_f64(ptr, idx, val)` | Write a number field |
| `mem_struct_get_ptr(ptr, idx)` | Read a heap field |
| `mem_struct_set_owned(ptr, idx, val)` | Write a heap field — move if owned, copy if borrowed |

### 5.6.3 Cleanup

| Function | Purpose |
|----------|---------|
| `mem_free(ptr)` | Free a tracked allocation. Recursive. Idempotent. |

### 5.6.4 Diagnostics

| Function | Purpose |
|----------|---------|
| `mem_stats()` | (allocs, frees, live_bytes) |
| `mem_reset()` | Zero counters |
| `mem_assert_clean()` | Panic if allocs != frees |

---

## 5.7 Type Identity

Every struct allocation carries a **type tag** — a deterministic u16 hash of the struct name. Two structs are equal only if they have the same type tag AND all fields match.

---

## 5.8 Research Basis

- **Lobster** (van Oortmerssen) — automatic ownership inference
- **Austral** (Borretti) — second-class references, ~600 LOC checker
- **Mojo** (Lattner) — last-use destruction
- **Koka/Perceus** (PLDI 2021) — compile-time RC optimization
- **Hylo** (Abrahams, Racordon) — mutable value semantics, second-class references
