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
Package name: rocalang
```

Compiled Roca programs import from this package:

```javascript
import { RocaMath, RocaFs, RocaHttp } from "rocalang";
```

### 4.2.2 Naming Convention

All stdlib exports use the `Roca` prefix to avoid collisions with JS globals:

| Roca Contract | JS Export Name |
|---------------|---------------|
| `Math` | `RocaMath` |
| `JSON` | `RocaJSON` |
| `Map` | `RocaMap` |
| `Fs` | `RocaFs` |
| `Http` | `RocaHttp` |
| `Url` | `RocaUrl` |
| `Crypto` | `RocaCrypto` |
| `Encoding` | `RocaEncoding` |
| `Time` | `RocaTime` |
| `Path` | `RocaPath` |
| `Char` | `RocaChar` |
| `NumberParse` | `RocaNumberParse` |
| `Process` | `RocaProcess` |

The compiler MUST emit `RocaMath.floor(n)` when the Roca source says `Math.floor(n)`. The user writes clean contract names; the `Roca` prefix is a compilation detail.

### 4.2.3 Installation

The compiler SHOULD auto-install `rocalang` in the output directory's `package.json` during `roca build`. Alternatively, users MAY install it manually:

```bash
npm install rocalang
```

---

## 4.3 JS Compilation Output

### 4.3.1 Stdlib Imports

When a Roca file imports from `std::`, the compiled JS MUST emit a single import from `rocalang`:

```roca
import { Math } from std::math
import { Fs } from std::fs
```

Compiles to:

```javascript
import { RocaMath, RocaFs } from "rocalang";
```

### 4.3.2 Relative Imports

Relative `.roca` imports compile to relative `.js` imports:

```roca
import { User } from "./types.roca"
```

Compiles to:

```javascript
import { User } from "./types.js";
```

### 4.3.3 Identifier Mapping

The compiler MUST replace stdlib contract names with their `Roca`-prefixed equivalents in all emitted JS:

```roca
const result = Math.floor(3.7)
```

Compiles to:

```javascript
const result = RocaMath.floor(3.7);
```

This mapping applies ONLY to stdlib contracts imported via `std::`. User-defined extern contracts keep their original names.

---

## 4.4 Polyfill Strategy

The `rocalang` package MUST provide implementations that work across environments:

### 4.4.1 Node.js / Bun

All modules available. Uses native APIs:
- `fs` → `node:fs`
- `crypto` → `node:crypto`
- `fetch` → native fetch
- `URL` → native URL
- `process` → native process

### 4.4.2 Browser

Most modules available. Exceptions:
- `Fs` — NOT available (no filesystem). Compiler SHOULD warn on import.
- `Process` — NOT available (no process control). Compiler SHOULD warn on import.
- `Http` → native `fetch`
- `Crypto` → `crypto.subtle`
- `URL` → native `URL`

### 4.4.3 Embedded V8 (roca build tests)

Limited environment. The compiler injects polyfills before test execution:

| API | Source |
|-----|--------|
| `console` | Polyfill (captures output for test parsing) |
| `TextEncoder/Decoder` | Polyfill |
| `atob/btoa` | Polyfill |
| `URL` | Rust bridge via `deno_core` ops |
| `crypto` | Rust bridge via `deno_core` ops |
| `setTimeout` | Polyfill |
| `fetch` | Not available — tests use auto-stubs |
| `fs` | Not available — tests use auto-stubs |

For embedded test mode, the compiler MUST inline the `rocalang` runtime (with `export` stripped) before the test code, rather than using an `import` statement.

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
