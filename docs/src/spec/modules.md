# 4. Module System

**Status:** DRAFT — Decisions made, implementation in progress.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

---

## 4.1 Import Syntax

```
Import = 'import' '{' Ident (',' Ident)* '}' 'from' ImportSource
ImportSource = StringLit | 'std' '::' Ident
```

### 4.1.1 Stdlib Imports

```roca
import { Math } from std::math
import { Fs } from std::fs
import { Http } from std::http
import { JSON } from std::json
```

Stdlib imports reference contracts defined in the standard library. The module name after `std::` maps directly to a stdlib module. The compiler resolves the contract definition for type checking and the runtime implementation for execution.

### 4.1.2 Relative File Imports

```roca
import { UserProfile } from "./types.roca"
import { DatabaseClient } from "./db/client.roca"
```

Relative imports reference other `.roca` files in the same project. The path MUST be relative (starting with `./` or `../`) and MUST use the `.roca` extension. The compiler resolves the path relative to the importing file's directory.

### 4.1.3 User Extern Contracts

User-defined extern contracts are NOT imported. They are declared as types and passed as function parameters:

```roca
/// User defines the contract shape
pub extern contract KV {
    get(key: String) -> String, err {
        err not_found = "key not found"
    }
    put(key: String, value: String) -> Ok, err {
        err write_failed = "write failed"
    }
}

/// Functions receive instances as parameters
pub fn get_user(kv: KV, id: String) -> String {
    const data = kv.get("user:" + id)
    return data
crash {
    kv.get -> fallback("not found")
}
test {
    self(KV, "123") == ""
}}
```

The caller provides the real implementation at runtime. In tests, the compiler auto-generates stubs from the contract's type signatures.

---

## 4.2 Runtime Package

### 4.2.1 Package Identity

The stdlib runtime is published as an npm package:

```
Package name: @rocalang/runtime
```

Compiled Roca programs import from this package:

```javascript
import { RocaMath, RocaFs, RocaHttp } from "@rocalang/runtime";
```

### 4.2.2 Naming Convention

Stdlib exports use the same names as the Roca contracts. ES module imports are lexically scoped, so there are no global collisions:

```javascript
// @rocalang/runtime exports Math, JSON, Map, etc.
// These shadow globals within the importing module — no conflict
import { Math, JSON, Fs } from "@rocalang/runtime";

Math.floor(3.7);   // Calls @rocalang/runtime's Math, not globalThis.Math
JSON.parse(text);  // Calls @rocalang/runtime's JSON, not globalThis.JSON
```

The compiler emits `Math.floor(n)` exactly as written in Roca source. No prefixing or renaming needed.

### 4.2.3 Installation

The compiler MUST add `@rocalang/runtime` as a dependency in the output `package.json` during `roca build`:

```json
{
  "dependencies": {
    "@rocalang/runtime": "^0.3.0"
  }
}
```

### 4.2.4 The `wrap` Utility

The runtime MUST export a `wrap` function that converts plain JS functions to Roca's error protocol:

```javascript
import { wrap } from "@rocalang/runtime";

// Plain JS function — throws on error
function fetchUser(id) {
    if (!id) throw new Error("missing id");
    return { name: "cam" };
}

// Wrapped — returns { value, err } protocol
export const getUser = wrap(fetchUser);
// Success: { value: { name: "cam" }, err: null }
// Error:   { value: null, err: { name: "Error", message: "missing id" } }
```

The `wrap` function MUST:
1. Call the wrapped function inside a try/catch
2. On success: return `{ value: result, err: null }`
3. On exception: return `{ value: null, err: { name: e.name || "Error", message: e.message } }`
4. Support async functions: if the wrapped function returns a Promise, `wrap` MUST return an async function that awaits the Promise before wrapping

This is how users bridge existing JS code into Roca's error protocol without rewriting it.

---

## 4.3 JS Compilation Output

### 4.3.1 Stdlib Imports

When a Roca file imports from `std::`, the compiled JS MUST emit an import from `@rocalang/runtime`:

```roca
import { Math } from std::math
import { Fs } from std::fs
```

Compiles to:

```javascript
import { Math, Fs } from "@rocalang/runtime";
```

All stdlib imports MUST be consolidated into a single `import` statement from `@rocalang/runtime`.

### 4.3.2 Relative Imports

Relative `.roca` imports compile to relative `.js` imports:

```roca
import { User } from "./types.roca"
```

Compiles to:

```javascript
import { User } from "./types.js";
```

### 4.3.3 No Identifier Mapping

Contract names are emitted as-is. No renaming or prefixing:

```roca
const result = Math.floor(3.7)
```

Compiles to:

```javascript
const result = Math.floor(3.7);
```

ES module scoping ensures the imported `Math` shadows `globalThis.Math` within the module. No `Roca` prefix needed.

---

## 4.4 Runtime Environments

The `@rocalang/runtime` package MUST work in two environments: Node.js/Bun and browsers.

### 4.4.1 Node.js / Bun

All modules available. The runtime uses native APIs:

| Stdlib | Implementation |
|--------|---------------|
| Fs | `node:fs` (readFileSync, writeFileSync, etc.) |
| Process | `process` global |
| Crypto | `node:crypto` |
| Http | native `fetch` |
| URL | native `URL` |
| Math, Path, Char, etc. | Pure JS (no platform dependency) |

### 4.4.2 Browser

Most modules available. The runtime detects the environment and adapts:

| Stdlib | Implementation |
|--------|---------------|
| Http | native `fetch` |
| Crypto | `crypto.subtle` |
| URL | native `URL` |
| Math, Path, Char, etc. | Pure JS |
| Fs | **NOT available** — compiler SHOULD warn |
| Process | **NOT available** — compiler SHOULD warn |

### 4.4.3 Test Execution

Testing runs **natively** via the Cranelift JIT compiler. There is no JS test runner.

```bash
roca build    # tests run natively via Cranelift JIT → if pass, emits JS
roca test     # tests only, no JS output
```

The compiler IS the test engine:

1. Parse and check `.roca` source
2. Compile to Cranelift IR via JIT
3. Execute test blocks and battle tests natively
4. If all pass → emit JS output with `@rocalang/runtime` imports
5. If any fail → no JS emitted, report failures

This means:
- No `deno_core`, no V8, no polyfills, no Rust bridges
- Tests run at native speed
- The compiler guarantees JS output is correct because it proved the logic natively
- `@rocalang/runtime` is a production dependency only — never used during testing

---

## 4.5 Stdlib Modules

| Module | Import Path | Contract | Description |
|--------|------------|----------|-------------|
| Math | `std::math` | Math | floor, ceil, round, abs, sqrt, pow, min, max |
| Char | `std::char` | Char | fromCode, isDigit, isLetter, isWhitespace |
| NumberParse | `std::parse` | NumberParse | parse(String) -> Number |
| Path | `std::path` | Path | join, dirname, basename, extension |
| Map | `std::map` | Map | new, get, set, has, delete, keys, values, size |
| JSON | `std::json` | JSON | parse, stringify, get, getString, etc. |
| Encoding | `std::encoding` | Encoding | encode, decode, btoa, atob |
| Http | `std::http` | Http | get, post, put, delete, status, text, json |
| Url | `std::url` | Url | parse, host, pathname, getParam, isValid |
| Crypto | `std::crypto` | Crypto | randomUUID, sha256, sha512 |
| Time | `std::time` | Time | now, parse |
| Fs | `std::fs` | Fs | readFile, writeFile, exists, readDir |
| Process | `std::process` | Process | args, env, cwd, exit |
