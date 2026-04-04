# Memory Model

Roca enforces ownership at compile time. Same source compiles to JavaScript (GC) or native binary (deterministic memory). The rules are the same on both targets.

---

## Rules

### 1. `const` owns

Every value must be bound to a `const`. The `const` is responsible for the value's lifetime.

```roca
const user = User.new("alice", 30)
const count = 42
```

### 2. `let` borrows from `const`

`let` is a read-only borrow. It must derive from an existing `const`, not create new values.

```roca
const user = User.new("alice", 30)
let name = user.name     // borrow a field
let borrowed = user       // borrow the whole struct
```

### 3. Borrow before passing to `b`

A `const` cannot be passed directly to a `b` parameter. Create a `let` first.

```roca
const file = open("data.txt")
let ref = file
process(ref)              // process(b file) borrows it
```

### 4. Passing to `o` is a move

Passing a `const` to an `o` parameter transfers ownership. The value is dead after.

```roca
const file = open("data.txt")
consume(file)             // file is gone
// file cannot be used here
```

### 5. Parameters declare `o` or `b`

Every parameter states its intent. No ambiguity.

```roca
fn process(b config: Config, o data: Data) -> Result
```

### 6. Return values are owned

Functions return owned values. Returning a borrowed struct is a compile error — copy first.

### 7. Containers copy borrows

Storing a borrowed value in a struct field or array automatically copies it. The compiler emits a note (E-OWN-007).

---

## Second-Class References

Borrowed values (`let`) cannot be stored in struct fields or returned from functions. They exist only at function call boundaries. This eliminates the need for lifetime annotations.

## Last-Use Destruction

Owned values are freed at their last use, not at scope exit. On the native target, this means `mem_free` is called immediately after the last read. On JS, the GC handles it.

## Dual Target

| Operation | Native | JS |
|-----------|--------|-----|
| Allocation | `mem_struct_new` | `new Object()` |
| Free | `mem_free` at last use | GC |
| Move | Ownership transfers | Use-after-move = compile error |
| Borrow | Pointer passed | Reference passed |
| Struct field store | `mem_struct_set_owned` (copy if borrowed) | `obj.field = val` |

## Runtime API (roca-mem)

| Function | Purpose |
|----------|---------|
| `mem_string_new(src)` | Copy C string into tracked allocation |
| `mem_struct_new(n, type_id)` | Allocate struct with type identity |
| `mem_struct_get_f64(ptr, idx)` | Read number field |
| `mem_struct_set_f64(ptr, idx, val)` | Write number field |
| `mem_struct_get_ptr(ptr, idx)` | Read heap field |
| `mem_struct_set_owned(ptr, idx, val)` | Write heap field — move or copy |
| `mem_free(ptr)` | Recursive, idempotent cleanup |
| `mem_type_id(ptr)` | Get struct type tag |
| `mem_stats()` | (allocs, frees, live_bytes) |
| `mem_assert_clean()` | Panic if leaks detected |
