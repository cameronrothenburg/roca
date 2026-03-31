# 4. Module System

**Status:** STUB — This section is under active design. The module system is being reworked for v0.3.0.

## 4.1 Open Questions

The following decisions are pending:

1. **Import path format** — Should `import { Math } from std::math` stay flat, or use nested paths like `import { Math } from std::core::math`?

2. **JS runtime packaging** — Should stdlib JS implementations be:
   - Inlined into compiled output (current, causes naming collisions)
   - Provided as an `@roca/runtime` npm package
   - Something else

3. **Global name collisions** — Stdlib contracts like `Math`, `JSON`, `Map` clash with JS globals when inlined. Solutions under consideration:
   - Prefix all stdlib JS exports with `Roca` (e.g., `RocaMath`)
   - Runtime package with proper ES module isolation
   - Scoped IIFE wrapping

4. **Polyfill strategy** — How do stdlib implementations work across V8 embed, Node/Bun, and browser environments?

5. **User extern contracts** — How do user-defined extern contracts (passed as function params) interact with the module system?

## 4.2 Current Behavior (Pre-Spec)

```roca
// Stdlib import — JS wrapper inlined into output
import { Math } from std::math

// Relative file import — becomes ES6 import
import { User } from "./user.roca"

// Base stdlib — primitives, no JS needed
import { } from std
```

### Resolution

- `std::module` → searches `packages/stdlib/{module}.js` and subdirectories
- `"./path.roca"` → replaced with `"./path.js"` in output
- `std` (bare) → no output, primitives are built-in

### Known Issues

- Stdlib wrappers use `const Math = { ... }` which clashes with `globalThis.Math` in V8
- IIFE-wrapped modules (json, encoding, http, url, crypto) lost their exports during refactoring
- `time/time.roca` double-nested path breaks resolver
- No `@roca/runtime` package exists yet
- `fs.js` imports from `node:fs` which doesn't work in V8 embed

## 4.3 Planned Design

*To be defined. This section will specify:*

- Import syntax and path resolution rules
- Stdlib package structure and naming
- Runtime package format and installation
- Polyfill requirements per environment
- User module conventions
