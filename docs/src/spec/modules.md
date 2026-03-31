# 4. Module System

**Status:** DRAFT — Decisions made, implementation in progress.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

---

## 4.1 Project Structure

Every Roca project MUST have a `roca.toml` file at its root. This file defines the project identity and configuration.

```toml
name = "my-app"
version = "1.0.0"
```

The compiler MUST reject any directory build that does not contain a `roca.toml`. Single-file builds MAY work without one, but directory builds MUST NOT.

### 4.1.1 Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | String | Package name (used in output `package.json`) |
| `version` | String | Semver version |

### 4.1.2 Source Directory

Roca source files live in `src/` by default. The compiler MUST search for `.roca` files in the `src/` directory relative to `roca.toml`.

```
my-app/
  roca.toml
  src/
    main.roca
    types.roca
    db/
      client.roca
```

---

## 4.2 Imports

```
Import = 'import' '{' Ident (',' Ident)* '}' 'from' StringLit
```

### 4.2.1 Standard Library

Stdlib contracts (Math, Fs, Http, JSON, etc.) are NOT imported. They are built into the compiler. The compiler recognizes stdlib contract names and emits the correct runtime calls automatically.

```roca
/// No import needed — Math is a known stdlib contract
pub fn process(n: Number) -> Number {
    return Math.floor(n)
test {
    self(3.7) == 3
}}
```

The compiler MUST know all stdlib contract names and their method signatures at compile time. Stdlib contracts are defined in `packages/stdlib/**/*.roca` and embedded in the compiler binary.

### 4.2.2 Reserved Names

User code MUST NOT define contracts, structs, or enums with the same names as stdlib contracts. The following names are reserved:

`String`, `Number`, `Bool`, `Array`, `Map`, `Optional`, `Bytes`, `Buffer`, `Math`, `JSON`, `Fs`, `Http`, `Url`, `Crypto`, `Encoding`, `Time`, `Path`, `Char`, `NumberParse`, `Process`, `Loggable`, `Serializable`, `Deserializable`

A conforming compiler MUST reject user-defined types that collide with these names with diagnostic `reserved-name`.

Users MAY extend stdlib contracts via `satisfies`:

```roca
/// Extending a stdlib contract — allowed
MyStruct satisfies Loggable {
    fn toLog() -> String {
        return self.name
    test { }
    }
}
```

### 4.2.3 File Imports

The `import` statement brings names from other `.roca` files into scope:

```roca
import { UserProfile } from "./types.roca"
import { DatabaseClient } from "./db/client.roca"
```

Import paths MUST be relative (starting with `./` or `../`) and MUST use the `.roca` extension. The compiler resolves paths relative to the importing file's directory.

### 4.2.4 User Extern Contracts

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

## 4.3 Runtime Package

### 4.2.1 Package Identity

The stdlib runtime is published as an npm package:

```
Package name: @rocalang/runtime
```

The runtime exports a single `roca` object containing all stdlib implementations:

```javascript
import roca from "@rocalang/runtime";

roca.Math.floor(3.7);
roca.Fs.readFile(path);
roca.Http.get(url);
```

### 4.2.2 Runtime Map

The runtime provides a map of stdlib implementations. Each entry is a wrapped version of the platform's native API (or a pure JS implementation where no native API exists):

```javascript
// @rocalang/runtime internals
export default {
    Math: wrap({ floor: globalThis.Math.floor, ceil: globalThis.Math.ceil, ... }),
    JSON: wrap({ parse: globalThis.JSON.parse, stringify: globalThis.JSON.stringify, ... }),
    Fs: wrap({ readFile: readFileSync, writeFile: writeFileSync, ... }),  // Node only
    Http: wrap({ get: fetch, post: fetch, ... }),
    // ...
};
```

The `wrap` function converts each method to Roca's `{ value, err }` protocol. The runtime detects the environment and only populates what's available — `Fs` exists on Node/Bun, not in browsers.

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

## 4.4 JS Compilation Output

### 4.3.1 Stdlib Imports

When a Roca file uses stdlib contracts, the compiled JS MUST import the runtime and access stdlib via the `roca` map:

```roca
pub fn process() -> Number {
    const data = Fs.readFile("config.json")
    return Math.floor(data.length)
}
```

Compiles to:

```javascript
import roca from "@rocalang/runtime";

export function process() {
    const _data_tmp = roca.Fs.readFile("config.json");
    const _data_err = _data_tmp.err;
    // ... crash handling ...
    const data = _data_tmp.value;
    return roca.Math.floor(data.length);
}
```

The compiler MUST:
1. Emit a single `import roca from "@rocalang/runtime"` at the top
2. Replace all stdlib contract references with `roca.ContractName`
3. Only emit the runtime import if the file uses stdlib contracts

### 4.3.2 Relative Imports

Relative `.roca` imports compile to relative `.js` imports unchanged:

```roca
import { User } from "./types.roca"
```

Compiles to:

```javascript
import { User } from "./types.js";
```

### 4.3.3 Identifier Mapping

The compiler MUST prefix stdlib contract names with `roca.` in all emitted JS:

| Roca source | JS output |
|-------------|-----------|
| `Math.floor(n)` | `roca.Math.floor(n)` |
| `Fs.readFile(path)` | `roca.Fs.readFile(path)` |
| `Http.get(url)` | `roca.Http.get(url)` |

This mapping applies ONLY to contracts imported via `std::`. User-defined types and relative imports are emitted as-is.

---

## 4.5 Runtime Environments

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

### 4.5.2 Browser

Most modules available. The runtime detects the environment and adapts:

| Stdlib | Implementation |
|--------|---------------|
| Http | native `fetch` |
| Crypto | `crypto.subtle` |
| URL | native `URL` |
| Math, Path, Char, etc. | Pure JS |
| Fs | **NOT available** |
| Process | **NOT available** |

### 4.5.3 Unavailable Functions

Importing the runtime MUST NOT throw. The runtime MUST only return an error when a function that is not implemented on the current platform is actually called.

```javascript
// Browser runtime — Fs.readFile throws
roca.Fs.readFile("config.json");
// → { value: null, err: { name: "platform", message: "Fs.readFile is not available in browsers" } }
```

The error name MUST be `"platform"`. The message MUST identify the function and the environment. This ensures crash blocks can handle platform unavailability:

```roca
pub fn loadConfig(path: String) -> String {
    const data = Fs.readFile(path)
    return data
crash {
    Fs.readFile -> fallback("{}")
}
}
```

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

## 4.6 Stdlib Modules

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
