# 8. Runtime

This section defines the runtime requirements for Roca programs. The runtime provides stdlib implementations, memory management, error protocol support, and environment polyfills for each compilation target.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

---

## 8.1 Runtime Architecture

Roca programs depend on a target-specific runtime for stdlib implementations and core services. A conforming Roca implementation MUST provide a runtime for each supported target.

| Target | Runtime Form | Distribution |
|--------|-------------|-------------|
| JavaScript | `rocalang` npm package | Installed as dependency in output project |
| Native | Linked extern "C" functions | Registered with Cranelift JIT at compile time |

The runtime is not user-authored code. It is provided by the Roca toolchain and MUST NOT be modified by application developers.

### 8.1.1 Runtime Versioning

The runtime version MUST match the compiler version. A conforming compiler MUST reject a runtime whose major version differs from its own. Minor version mismatches SHOULD produce a warning but MUST NOT be rejected.

---

## 8.2 JS Runtime Requirements

A conforming JS runtime MUST provide the following:

### 8.2.1 Stdlib Contract Implementations

The runtime MUST export implementations for all stdlib contracts. Each stdlib module (e.g., `std::math`, `std::fs`, `std::http`, `std::json`) MUST have a corresponding export in `@roca/runtime`.

```javascript
// @roca/runtime exports
export { RocaMath } from "./math.js";
export { RocaFs } from "./fs.js";
export { RocaHttp } from "./http.js";
export { RocaJson } from "./json.js";
```

The emitter resolves `import { floor } from std::math` to `import { RocaMath } from "@roca/runtime"` and rewrites call sites accordingly.

### 8.2.2 Error Tuple Protocol

All error-returning functions MUST use the following object shape:

```javascript
// Success
{ value: result, err: null }

// Error
{ value: null, err: { name: "error_name", message: "description" } }
```

The runtime MUST provide helper functions for constructing these tuples:

```javascript
export function ok(value) {
    return { value, err: null };
}

export function error(name, message) {
    return { value: null, err: { name, message } };
}
```

### 8.2.3 Async Support

The runtime MUST support Promise-based async for `wait`, `waitAll`, and `waitFirst`:

- `wait` calls MUST resolve to `await`.
- `waitAll` calls MUST resolve to `Promise.all`.
- `waitFirst` calls MUST resolve to `Promise.race`.

The runtime MUST NOT introduce its own async primitives. Standard JavaScript Promises are sufficient.

---

## 8.3 Native Runtime Functions

The native runtime provides extern "C" functions that are registered with the Cranelift JIT module at compile time. Each function MUST use C calling conventions and operate on Roca's reference-counted heap objects.

### 8.3.1 String Operations

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_string_alloc` | `(ptr: *const u8, len: i64) -> *mut u8` | Allocate a new RC string from bytes |
| `roca_string_concat` | `(a: *const u8, b: *const u8) -> *mut u8` | Concatenate two strings |
| `roca_string_trim` | `(s: *const u8) -> *mut u8` | Trim whitespace from both ends |
| `roca_string_to_upper` | `(s: *const u8) -> *mut u8` | Convert to uppercase |
| `roca_string_to_lower` | `(s: *const u8) -> *mut u8` | Convert to lowercase |
| `roca_string_slice` | `(s: *const u8, start: i64, end: i64) -> *mut u8` | Extract substring |
| `roca_string_split` | `(s: *const u8, delim: *const u8) -> *mut u8` | Split into array of strings |
| `roca_string_includes` | `(s: *const u8, search: *const u8) -> i8` | Check if string contains substring |
| `roca_string_starts_with` | `(s: *const u8, prefix: *const u8) -> i8` | Check prefix |
| `roca_string_ends_with` | `(s: *const u8, suffix: *const u8) -> i8` | Check suffix |
| `roca_string_len` | `(s: *const u8) -> i64` | Return character count (not byte count) |
| `roca_string_replace` | `(s: *const u8, from: *const u8, to: *const u8) -> *mut u8` | Replace occurrences |

### 8.3.2 Array Operations

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_array_new` | `(capacity: i64) -> *mut u8` | Allocate a new RC array |
| `roca_array_push` | `(arr: *mut u8, item: *const u8) -> ()` | Append an element |
| `roca_array_get` | `(arr: *const u8, index: i64) -> *const u8` | Get element by index |
| `roca_array_len` | `(arr: *const u8) -> i64` | Return element count |
| `roca_array_join` | `(arr: *const u8, sep: *const u8) -> *mut u8` | Join elements with separator |

### 8.3.3 Struct Operations

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_struct_alloc` | `(field_count: i64) -> *mut u8` | Allocate a new RC struct |
| `roca_struct_get` | `(s: *const u8, index: i64) -> *const u8` | Get field by index |
| `roca_struct_set` | `(s: *mut u8, index: i64, value: *const u8) -> ()` | Set field by index |

### 8.3.4 Map Operations

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_map_new` | `() -> *mut u8` | Allocate a new RC map |
| `roca_map_get` | `(m: *const u8, key: *const u8) -> *const u8` | Get value by key |
| `roca_map_set` | `(m: *mut u8, key: *const u8, value: *const u8) -> ()` | Set key-value pair |
| `roca_map_has` | `(m: *const u8, key: *const u8) -> i8` | Check if key exists |
| `roca_map_delete` | `(m: *mut u8, key: *const u8) -> ()` | Remove key-value pair |
| `roca_map_keys` | `(m: *const u8) -> *mut u8` | Return array of keys |
| `roca_map_values` | `(m: *const u8) -> *mut u8` | Return array of values |
| `roca_map_size` | `(m: *const u8) -> i64` | Return number of entries |
| `roca_map_free` | `(m: *mut u8) -> ()` | Free the map and its contents |

### 8.3.5 Reference Counting

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_rc_alloc` | `(size: i64) -> *mut u8` | Allocate with 16-byte RC header |
| `roca_rc_retain` | `(ptr: *mut u8) -> ()` | Increment refcount |
| `roca_rc_release` | `(ptr: *mut u8) -> ()` | Decrement refcount; free if zero |

### 8.3.6 I/O Operations

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_read_file` | `(path: *const u8) -> *mut u8` | Read file contents as string |
| `roca_write_file` | `(path: *const u8, data: *const u8) -> ()` | Write string to file |
| `roca_exists` | `(path: *const u8) -> i8` | Check if path exists |
| `roca_read_dir` | `(path: *const u8) -> *mut u8` | List directory entries as array |

### 8.3.7 Math Operations

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_math_floor` | `(n: f64) -> f64` | Round down |
| `roca_math_ceil` | `(n: f64) -> f64` | Round up |
| `roca_math_round` | `(n: f64) -> f64` | Round to nearest integer |
| `roca_math_abs` | `(n: f64) -> f64` | Absolute value |
| `roca_math_sqrt` | `(n: f64) -> f64` | Square root |
| `roca_math_pow` | `(base: f64, exp: f64) -> f64` | Exponentiation |
| `roca_math_min` | `(a: f64, b: f64) -> f64` | Minimum of two values |
| `roca_math_max` | `(a: f64, b: f64) -> f64` | Maximum of two values |

### 8.3.8 Character Operations

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_char_from_code` | `(code: i64) -> *mut u8` | Create string from Unicode code point |
| `roca_char_is_digit` | `(c: *const u8) -> i8` | Check if character is a digit |
| `roca_char_is_letter` | `(c: *const u8) -> i8` | Check if character is a letter |
| `roca_char_is_whitespace` | `(c: *const u8) -> i8` | Check if character is whitespace |
| `roca_char_is_alphanumeric` | `(c: *const u8) -> i8` | Check if character is alphanumeric |

### 8.3.9 Concurrency Operations

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_sleep` | `(ms: i64) -> ()` | Sleep for milliseconds |
| `roca_wait_all` | `(fns: *const u8, count: i64) -> *mut u8` | Execute functions in parallel, collect results |
| `roca_wait_first` | `(fns: *const u8, count: i64) -> *mut u8` | Execute functions in parallel, return first result |

---

## 8.4 Polyfill Requirements

Different JavaScript environments provide different global APIs. The runtime MUST polyfill missing APIs to ensure consistent behavior.

| API | V8 Embed | Node/Bun | Browser |
|-----|----------|----------|---------|
| `console.log` | Polyfill (capture mode) | Native | Native |
| `TextEncoder` / `TextDecoder` | Polyfill | Native | Native |
| `atob` / `btoa` | Polyfill | Native | Native |
| `URL` / `URLSearchParams` | Bridge (Rust op) | Native | Native |
| `crypto.randomUUID` | Bridge (Rust op) | Native | Native |
| `crypto.subtle` | Bridge (Rust op) | Native | Native |
| `fetch` | Not available | Native | Native |
| `fs` (readFile, etc.) | Not available | Native | Not available |
| `process.exit` | Polyfill | Native | Not available |
| `setTimeout` | Polyfill | Native | Native |

### 8.4.1 Polyfill

A **polyfill** is a pure JavaScript implementation of the API that behaves identically to the native version. The runtime MUST install polyfills before any user code executes.

### 8.4.2 Bridge

A **bridge** is a JavaScript shim that delegates to a Rust operation via V8 bindings. Bridges MUST be registered before any user code executes. Bridge operations MUST be synchronous from the JavaScript caller's perspective (the Rust side handles any async work internally).

### 8.4.3 Not Available

APIs marked as **Not available** are not supported in that environment. Roca source that uses stdlib contracts backed by unavailable APIs MUST produce a compile-time error (`unsupported-environment`) when targeting that environment. The compiler MUST NOT silently omit functionality.

### 8.4.4 Capture Mode (V8 Embed)

In V8 embed mode, `console.log` MUST NOT write to stdout. Instead, it MUST capture logged values into an internal buffer. The host application MAY retrieve captured logs via the runtime API. This enables testing without side effects.

---

## 8.5 Memory Tracking (Native)

The native runtime MUST maintain thread-local memory counters for diagnostics and leak detection.

### 8.5.1 Counters

The following counters MUST be tracked per thread:

| Counter | Description |
|---------|-------------|
| `allocs` | Total number of `rc_alloc` calls |
| `frees` | Total number of frees (refcount reached zero) |
| `retains` | Total number of `rc_retain` calls |
| `releases` | Total number of `rc_release` calls |
| `live_bytes` | Current total bytes allocated and not yet freed |

### 8.5.2 Leak Detection in Tests

Tests MAY assert that `allocs == frees` after execution to verify that no memory was leaked. This assertion is OPTIONAL and is not enforced by the compiler. A conforming test runner SHOULD report a warning (not an error) when `allocs != frees` after a test completes.

### 8.5.3 Debug Mode

The runtime MUST support a debug mode activated via `MEM.set_debug(true)`. When debug mode is enabled, the runtime MUST print a log line for every `rc_alloc` and every free, including:

- The pointer address.
- The allocation size in bytes.
- The current refcount (for frees, this is always 0).

Debug mode SHOULD be disabled by default. It MAY be enabled via an environment variable (`ROCA_MEM_DEBUG=1`) or programmatically.

---

## 8.6 Error Protocol

All error-returning functions MUST use a consistent error protocol across both targets.

### 8.6.1 JS Error Protocol

On the JavaScript target, errors MUST be represented as objects with `value` and `err` fields:

```javascript
// Success
{ value: result, err: null }

// Error
{ value: null, err: { name: "error_name", message: "description" } }
```

Rules:

- `value` MUST be `null` when `err` is non-null.
- `err` MUST be `null` when `value` is non-null.
- `err.name` MUST be a string matching the error name declared in the Roca source (e.g., `"not_found"`).
- `err.message` MUST be a string matching the error message declared in the Roca source.
- Callers MUST check `err` before accessing `value`. Accessing `value` when `err` is non-null is undefined behavior.

### 8.6.2 Native Error Protocol

On the native target, errors MUST be represented as a tagged return value:

```
// Function returns (value: T, err_tag: i8)
// err_tag = 0 -> success, value contains the result
// err_tag > 0 -> error, tag encodes the error variant
```

Rules:

- `err_tag` MUST be `0` for success.
- `err_tag` MUST be a positive integer for errors, where the value corresponds to the declaration order of errors in the function body (first error = 1, second error = 2, etc.).
- When `err_tag > 0`, the value field is undefined and MUST NOT be read.
- The compiler MUST generate a lookup table mapping `err_tag` values to error names and messages for diagnostic purposes.

### 8.6.3 Cross-Target Consistency

The error name and message strings MUST be identical on both targets for the same Roca source. A function that returns `err.not_found` with message `"user does not exist"` MUST produce:

- JS: `{ value: null, err: { name: "not_found", message: "user does not exist" } }`
- Native: `err_tag = 1` (if `not_found` is the first declared error), with lookup table entry `{ name: "not_found", message: "user does not exist" }`
