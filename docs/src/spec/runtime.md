# 8. Runtime

This section defines the runtime requirements for executing compiled Roca programs. The JS runtime (`@rocalang/runtime`) provides stdlib implementations. The native runtime provides linked functions for Cranelift JIT/AOT.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

---

## 8.1 Runtime Architecture

| Target | Runtime | Distribution |
|--------|---------|-------------|
| JavaScript | `@rocalang/runtime` npm package | Dependency in output `package.json` |
| Native | Linked `extern "C"` functions | Registered with Cranelift JIT at compile time |

The runtime is provided by the Roca toolchain and MUST NOT be modified by application developers.

### 8.1.1 Runtime Versioning

The runtime version MUST match the compiler's major version. Minor version mismatches SHOULD produce a warning.

---

## 8.2 JS Runtime (`@rocalang/runtime`)

### 8.2.1 Default Export

The runtime MUST export a single default object containing all stdlib implementations:

```javascript
import roca from "@rocalang/runtime";

roca.Math.floor(3.7);
roca.Fs.readFile("config.json");
roca.Http.get("https://api.example.com");
```

The runtime detects the platform and populates available implementations. Unavailable functions (e.g., `Fs` in browsers) MUST return a `platform` error when called — they MUST NOT throw on import.

### 8.2.2 Error Tuple Protocol

All error-returning functions MUST use this shape:

```javascript
// Success
{ value: result, err: null }

// Error
{ value: null, err: { name: "error_name", message: "description" } }
```

- `value` MUST be `null` when `err` is non-null
- `err` MUST be `null` when `value` is non-null
- `err.name` MUST match the error name declared in the Roca contract
- `err.message` MUST be a human-readable string

### 8.2.3 The `wrap`, `ok`, and `error` Utilities

The runtime MUST export the following named functions:

- `wrap` -- converts plain JS functions to the error tuple protocol (see below)
- `ok(value)` -- returns `{ value: value, err: null }` (convenience for constructing success tuples)
- `error(name, message)` -- returns `{ value: null, err: { name: name, message: message } }` (convenience for constructing error tuples)

```javascript
import { wrap, ok, error } from "@rocalang/runtime";

ok(42);           // { value: 42, err: null }
error("not_found", "user does not exist");
// { value: null, err: { name: "not_found", message: "user does not exist" } }
```

#### `wrap`

The `wrap` function converts plain JS functions to the error tuple protocol:

```javascript
import { wrap } from "@rocalang/runtime";

const safeReadFile = wrap(fs.readFileSync);
// Success: { value: "file contents", err: null }
// Error:   { value: null, err: { name: "Error", message: "ENOENT: ..." } }
```

`wrap` MUST:
1. Call the function inside try/catch
2. On success: return `{ value: result, err: null }`
3. On exception: return `{ value: null, err: { name: e.name, message: e.message } }`
4. Support async: if the function returns a Promise, wrap MUST await it

### 8.2.4 Platform Detection

The runtime MUST detect the environment and populate the stdlib map accordingly:

| Stdlib | Node.js / Bun | Browser |
|--------|---------------|---------|
| Math | Pure JS | Pure JS |
| Path | Pure JS (POSIX-normalized) | Pure JS (POSIX-normalized) |
| Char | Pure JS | Pure JS |
| JSON | `globalThis.JSON` | `globalThis.JSON` |
| Encoding | `TextEncoder/Decoder` | `TextEncoder/Decoder` |
| Http | native `fetch` | native `fetch` |
| Url | native `URL` | native `URL` |
| Crypto | `node:crypto` | `crypto.subtle` |
| Time | `Date` | `Date` |
| Fs | `node:fs` | Returns `platform` error |
| Process | `process` global | Returns `platform` error |
| Map | `globalThis.Map` | `globalThis.Map` |

### 8.2.5 Unavailable Functions

When a function is not available on the current platform, calling it MUST return:

```javascript
{ value: null, err: { name: "platform", message: "Fs.readFile is not available in this environment" } }
```

The runtime MUST NOT throw on import or initialization. It MUST only return the platform error when the unimplemented function is actually called.

---

## 8.3 Native Runtime Functions

The native runtime provides `extern "C"` functions registered with Cranelift JIT.

### 8.3.1 Naming Convention

```text
roca_{module}_{function}
```

### 8.3.2 Type Mapping

Note: `roca_string_len` and `roca_array_len` return `i64` (count), not `f64`. The caller converts to `f64` when the result is used as a Roca `Number`.

| Roca Type | Cranelift | C |
|-----------|-----------|---|
| Number | `types::F64` | `f64` |
| Bool | `types::I8` | `u8` |
| String | `types::I64` | `i64` (pointer) |
| Array | `types::I64` | `i64` (pointer) |
| Struct | `types::I64` | `i64` (pointer) |
| Map | `types::I64` | `i64` (pointer) |

### 8.3.3 String Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_string_new` | `(i64) -> i64` | Create RC string from C string |
| `roca_string_concat` | `(i64, i64) -> i64` | Concatenate |
| `roca_string_eq` | `(i64, i64) -> i8` | Equality |
| `roca_string_len` | `(i64) -> i64` | Character count |
| `roca_string_trim` | `(i64) -> i64` | Trim whitespace |
| `roca_string_to_upper` | `(i64) -> i64` | Uppercase |
| `roca_string_to_lower` | `(i64) -> i64` | Lowercase |
| `roca_string_includes` | `(i64, i64) -> i8` | Contains |
| `roca_string_starts_with` | `(i64, i64) -> i8` | Prefix check |
| `roca_string_ends_with` | `(i64, i64) -> i8` | Suffix check |
| `roca_string_slice` | `(i64, i64, i64) -> i64` | Substring |
| `roca_string_split` | `(i64, i64) -> i64` | Split to array |
| `roca_string_char_at` | `(i64, i64) -> i64` | Char at index |
| `roca_string_char_code_at` | `(i64, i64) -> f64` | Char code |
| `roca_string_index_of` | `(i64, i64) -> f64` | Find substring |
| `roca_string_from_f64` | `(f64) -> i64` | Number to string |

### 8.3.4 Collection Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_array_new` | `() -> i64` | Create empty array |
| `roca_array_push_f64` | `(i64, f64)` | Push number |
| `roca_array_get_f64` | `(i64, i64) -> f64` | Get number at index |
| `roca_array_len` | `(i64) -> i64` | Array length |
| `roca_array_join` | `(i64, i64) -> i64` | Join with separator |
| `roca_map_new` | `() -> i64` | Create empty map |
| `roca_map_set` | `(i64, i64, i64) -> i64` | Set key-value |
| `roca_map_get` | `(i64, i64) -> i64` | Get by key |
| `roca_map_has` | `(i64, i64) -> i8` | Key exists |
| `roca_map_delete` | `(i64, i64) -> i8` | Remove key |
| `roca_map_keys` | `(i64) -> i64` | All keys as array |
| `roca_map_values` | `(i64) -> i64` | All values as array |
| `roca_map_size` | `(i64) -> f64` | Entry count |
| `roca_map_free` | `(i64)` | Free map |

### 8.3.5 Memory Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_rc_alloc` | `(i64) -> i64` | Allocate with RC header |
| `roca_rc_retain` | `(i64)` | Increment refcount |
| `roca_rc_release` | `(i64)` | Decrement; free if zero |
| `roca_struct_alloc` | `(i64) -> i64` | Allocate struct |
| `roca_free_struct` | `(i64, i64)` | Free struct, cascade release |
| `roca_free_array` | `(i64)` | Free array |

### 8.3.6 Concurrency Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `roca_sleep` | `(f64)` | Sleep ms (blocking) |
| `roca_wait_all` | `(i64, i64) -> i64` | Parallel exec via tokio |
| `roca_wait_first` | `(i64, i64) -> f64` | Race via tokio mpsc |

---

## 8.4 Memory Model (Native)

### 8.4.1 RC Header Layout

```text
[refcount: i64][total_size: i64][payload...]
```

Pointer returned to Roca code points to payload (header + 16 bytes).

### 8.4.2 Ownership

| Binding | Semantics | On pass |
|---------|-----------|---------|
| `const` | Immutable, borrowed | Callee MUST NOT free |
| `let` | Mutable, moved | Caller MUST NOT access after passing |

### 8.4.3 Scope Cleanup

At function exit, all live heap variables MUST be released. The return value MUST be excluded from cleanup.

---

## 8.5 Error Protocol

### 8.5.1 JS Protocol

```javascript
{ value: result, err: null }           // success
{ value: null, err: { name, message } } // error
```

### 8.5.2 Native Protocol

```text
fn(args...) -> (value: T, err_tag: i8)
// err_tag = 0 → success
// err_tag > 0 → error variant (declaration order)
```

### 8.5.3 Cross-Target Consistency

Error names and messages MUST be identical on both targets for the same source.
