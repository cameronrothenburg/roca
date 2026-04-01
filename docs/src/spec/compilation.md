# 7. Compilation

This section defines the compilation model for Roca programs. Roca targets two backends -- JavaScript (via OXC AST builder) and native machine code (via Cranelift). Both targets MUST produce semantically equivalent behavior for the same source.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

---

## 7.1 Compilation Targets

A conforming Roca compiler MUST support the following targets:

| Target | Backend | Output | Use Case |
|--------|---------|--------|----------|
| **JavaScript** | OXC AST builder | ES module (`.js`) | Web, Node, Bun, Deno |
| **Native (JIT)** | Cranelift JIT | In-memory machine code | Tests, REPL |
| **Native (AOT)** | Cranelift AOT | Binary executable | Production servers, CLI tools |

The JavaScript target is the default. A conforming implementation MUST support `--target js` (or no flag) and `--target native`. The JIT vs AOT distinction for native targets is implementation-defined.

---

## 7.2 JavaScript Emission

The JS emitter MUST produce valid ES module output. Each `.roca` source file MUST compile to exactly one `.js` output file.

### 7.2.1 Output Structure

```javascript
// Stdlib imports resolved to runtime package
import { RocaMath } from "@roca/runtime";

// User imports preserved as relative paths
import { User } from "./user.js";

// Functions
export function add(a, b) {
    return a + b;
}
```

### 7.2.2 Declaration Mapping

The emitter MUST apply the following transformations:

| Roca | JavaScript |
|------|------------|
| `pub fn name(args) -> T` | `export function name(args)` |
| `fn name(args) -> T` | `function name(args)` |
| `const x = value` | `const x = value;` |
| `let x = value` | `let x = value;` |
| `pub struct Name { fields }` | `export class Name { constructor(fields) { ... } }` |
| `enum Name { K = v }` | `export const Name = { K: v };` |

### 7.2.3 Control Flow Mapping

| Roca | JavaScript |
|------|------------|
| `if cond { ... }` | `if (cond) { ... }` |
| `if cond { ... } else { ... }` | `if (cond) { ... } else { ... }` |
| `for item in list { ... }` | `for (const item of list) { ... }` |
| `while cond { ... }` | `while (cond) { ... }` |
| `match val { P => expr }` | `switch`/`if-else` chain (implementation-defined) |

### 7.2.4 Error Return Protocol

Functions that declare errors MUST emit the error tuple protocol. The emitter MUST wrap return values and error paths into `{ value, err }` objects.

```roca
pub fn parse(input: String) -> Number, err {
    err not_a_number = "input is not a valid number"
    return 42
}
```

Emits:

```javascript
export function parse(input) {
    if (/* constraint check */) {
        return { value: null, err: { name: "not_a_number", message: "input is not a valid number" } };
    }
    return { value: 42, err: null };
}
```

### 7.2.5 Crash Block Mapping

Crash blocks MUST emit try/catch with the declared strategy implementation:

| Strategy | JavaScript |
|----------|------------|
| `retry(n, ms)` | Loop with `n` attempts, `await sleep(ms)` between |
| `skip` | Return `undefined` / continue past the call |
| `halt` | `throw` (re-throw the caught error) |
| `fallback(value)` | Return the fallback value from the catch |
| `panic` | `console.error("PANIC:", _err); process.exit(1)` |

### 7.2.6 Async Mapping

| Roca | JavaScript |
|------|------------|
| `wait expr` | `await expr` |
| `waitAll(a, b, c)` | `await Promise.all([a, b, c])` |
| `waitFirst(a, b, c)` | `await Promise.race([a, b, c])` |

Functions containing `wait`, `waitAll`, or `waitFirst` MUST emit as `async function`. The emitter MUST propagate `async` to the function signature automatically -- Roca source does not declare `async` explicitly.

### 7.2.7 Constraint Mapping

Parameter and field constraints MUST emit as guard checks at function or constructor entry. A failing constraint MUST return an error tuple (for functions with `, err`) or throw (for functions without error returns).

```roca
pub fn clamp(value: Number, low: Number, high: Number) -> Number {
    return value
}
```

If `value` has constraints `min: 0, max: 100`, the emitter MUST produce:

```javascript
export function clamp(value, low, high) {
    if (value < 0 || value > 100) {
        throw new Error("constraint violation: value out of range");
    }
    return value;
}
```

### 7.2.8 Test Block Stripping

Test blocks MUST NOT appear in production JS output. The emitter MUST strip all `test { ... }` blocks during compilation. Test blocks are only evaluated during `roca build` and `roca test`.

---

## 7.3 Native Compilation

The native emitter compiles Roca source to Cranelift IR for execution as machine code.

### 7.3.1 Two-Pass Compilation

The native emitter MUST use a two-pass approach:

1. **Declaration pass**: Walk all top-level declarations and register function signatures (name, parameter types, return type) with the Cranelift module. This enables forward references and mutual recursion.
2. **Definition pass**: Walk all function bodies and emit Cranelift IR instructions for each.

### 7.3.2 Closure Compilation

Closures MUST be pre-compiled as top-level functions with deterministic hash-based names. The compiler MUST:

1. Lift the closure body to a module-level function.
2. Generate a unique name based on the enclosing function and closure position (e.g., `__closure_add_0x1a2b`).
3. Capture free variables as additional parameters prepended to the closure's parameter list.

### 7.3.3 Wait Expression Compilation

`wait` expressions MUST be pre-compiled as zero-argument functions suitable for tokio dispatch:

- `wait expr` compiles `expr` as a standalone function, then dispatches it via `tokio::spawn_blocking`.
- The calling function blocks on the result.

### 7.3.4 Forward References

The two-pass approach MUST support forward references. A function MAY call any other function declared in the same module, regardless of declaration order.

---

## 7.4 Memory Model (Native)

The native target uses reference counting for heap-allocated values. This section defines the memory layout and ownership rules.

### 7.4.1 Heap Object Layout

Every heap-allocated object MUST use a 16-byte header followed by the payload:

```text
[refcount: i64][total_size: i64][payload: ...]
```

- `refcount` starts at 1 on allocation.
- `total_size` includes the 16-byte header plus payload size.

### 7.4.2 Ownership Rules

| Binding | Semantics | On Pass |
|---------|-----------|---------|
| `const` | Immutable, borrowed | Callee receives a borrowed reference (no refcount change). Callee MUST NOT free. |
| `let` | Mutable, owned | Ownership transfers to callee. Caller MUST NOT access after passing. |

### 7.4.3 Scope Cleanup

At function exit, the runtime MUST release all live heap variables in the current scope. The return value MUST be excluded from cleanup -- it is the caller's responsibility to manage.

```roca
pub fn greet(name: String) -> String {
    const prefix = "hello "
    const result = prefix + name
    return result
    // prefix is released here; result is NOT (it is returned)
}
```

### 7.4.4 Reference Counting Operations

| Operation | Effect |
|-----------|--------|
| `rc_alloc(size)` | Allocates `size + 16` bytes, sets refcount to 1, returns pointer past header |
| `rc_retain(ptr)` | Increments refcount by 1 |
| `rc_release(ptr)` | Decrements refcount by 1; if zero, frees the allocation |

---

## 7.5 Concurrency (Native)

The native target uses tokio for async operations.

### 7.5.1 Wait

`wait expr` MUST compile to a blocking call that evaluates `expr` and returns the result. On the native target, this dispatches via tokio's runtime.

### 7.5.2 WaitAll

`waitAll(a, b, c)` MUST compile to parallel execution:

1. Each argument is compiled as a zero-arg function.
2. Each function is dispatched via `tokio::spawn_blocking`.
3. The caller blocks until all tasks complete.
4. Results are collected in declaration order and returned as an array.

### 7.5.3 WaitFirst

`waitFirst(a, b, c)` MUST compile to a race:

1. Each argument is compiled as a zero-arg function.
2. Each function is dispatched as a sender on a tokio mpsc channel.
3. The caller receives the first result from the channel.
4. Remaining tasks MAY be cancelled (implementation-defined).

### 7.5.4 Retry

`retry(n, ms)` in crash blocks MUST compile to a loop:

1. Attempt the operation.
2. On failure, sleep for `ms` milliseconds via `roca_sleep`.
3. Repeat up to `n` times.
4. If all attempts fail, propagate to the next strategy in the chain (or panic if none).

---

## 7.6 Target Equivalence

The same Roca source MUST produce semantically equivalent behavior on both the JavaScript and native targets. A conforming compiler MUST guarantee:

### 7.6.1 Number Precision

Both targets MUST use IEEE 754 64-bit floating point (`f64`) for all numeric values. Operations MUST produce identical results within IEEE 754 rounding rules.

### 7.6.2 String Encoding

Both targets MUST use UTF-8 encoding for all string values. String length MUST count Unicode scalar values, not bytes.

### 7.6.3 Error Protocol

Both targets MUST use the same error name and message structure. An `err.not_found` on JS and native MUST carry the same `name` string and `message` string.

### 7.6.4 Evaluation Order

Both targets MUST evaluate function arguments left-to-right. Both targets MUST evaluate `waitAll` arguments in parallel (or in unspecified order) and return results in declaration order.

### 7.6.5 Permitted Divergences

The following differences between targets are permitted and MUST NOT be considered conformance bugs:

- **Performance**: Execution speed MAY differ.
- **Memory usage**: Allocation patterns MAY differ (GC vs RC).
- **Stack depth**: Maximum recursion depth MAY differ.
- **Concurrency scheduling**: Task interleaving order MAY differ for `waitAll` and `waitFirst`.
