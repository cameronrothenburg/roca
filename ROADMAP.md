# Roadmap

## Where We Are (v0.2.0)

Roca is a working compiler with 770+ tests. It's being used in production on one project (LBS) with an AI writing Roca code alongside a human team.

### What works today

- **Compiler** — parser, type checker, JS emitter, 50+ rules
- **Error protocol** — `{value, err}` objects with crash block handling
- **Proof tests** — inline tests + fuzz testing, no JS emitted until they pass
- **TypeScript** — `.d.ts` generation with `RocaResult<T>` types
- **Extern contracts** — declare JS shapes, pass as explicit params
- **Stdlib** — primitives (String, Number, Bool, Array, Map, Bytes, Buffer, Optional)
- **Stdlib modules** — `std::json` with inline JS runtime wrapper
- **Tooling** — LSP server, `roca search`, `roca patterns`, `roca repl`
- **Documentation** — mdbook site, TextMate grammar, VS Code extension shell

### What's rough

- **Limited stdlib** — only `std::json` ships. Users write their own externs for everything else.
- **No escaped braces** — `\{` in strings isn't supported yet.
- **LSP needs testing** — may have stale patterns from earlier versions.
- **Single build target** — JS only, no direct TS emit.
- **No package registry** — can't `roca install` third-party packages.

---

## Where We Want to Be

### Stdlib: Web API contracts

The biggest gap. Roca should ship contracts for the JS ecosystem so users don't write boilerplate extern declarations. Each would be a `.roca` contract (type checking) + `.js` runtime wrapper (inline on import).

**Priority modules:**

| Module | What it wraps | Status |
|--------|--------------|--------|
| `std::json` | JSON.parse, JSON.stringify | ✓ Shipped |
| `std::http` | fetch, Response, Headers, Request | Planned |
| `std::crypto` | crypto.randomUUID, crypto.subtle | Planned |
| `std::time` | Date, setTimeout, setInterval, Date.now | Planned |
| `std::url` | URL, URLSearchParams | Planned |
| `std::encoding` | TextEncoder, TextDecoder, btoa, atob | Planned |
| `std::console` | console.log/error/warn (already built-in) | ✓ Built-in |

**Platform modules (separate packages):**

| Module | What it wraps | Status |
|--------|--------------|--------|
| `std::dom` | document, Element, Event, querySelector | Planned |
| `std::storage` | localStorage, sessionStorage | Planned |
| `std::workers` | Cloudflare Workers KV, D1, R2 | Planned |
| `std::node` | fs, path, process, child_process | Planned |

### Language features

| Feature | Description | Priority |
|---------|-------------|----------|
| Escaped braces | `\{` in strings treated as literal `{` | High |
| Type aliases | `type UserId = String` | Medium |
| Pattern destructuring | `const { name, email } = user` | Medium |
| Pipe operator | `value |> transform |> format` | Low |
| Module visibility | `pub(crate)` for internal-only exports | Low |

### Tooling

| Feature | Description | Priority |
|---------|-------------|----------|
| Package registry | `roca install` for third-party contracts | High |
| CI action | GitHub Action for `roca check` + `roca build` | Medium |
| Formatter | `roca fmt` — auto-format .roca files | Medium |
| Playground | Web-based REPL at roca-lang.dev | Low |
| VS Code extension publish | Marketplace listing | Medium |

### Quality

| Feature | Description | Priority |
|---------|-------------|----------|
| LSP refresh | Test against current patterns, fix stale completions | High |
| Error messages | Source line numbers, column highlighting | Medium |
| Incremental builds | Only recompile changed files | Medium |
| Watch mode | `roca build --watch` | Medium |

---

## Contributing

The compiler is AGPL-3.0 — modifications must stay open source.
Compiled output (JS) is MIT — use it however you want.

To add a stdlib module:
1. Create `packages/stdlib/{name}.roca` — extern contract with mock block
2. Create `packages/stdlib/{name}.js` — JS runtime wrapper
3. The compiler picks them up automatically by filename

See [Stdlib Modules](docs/src/integration/stdlib-modules.md) for details.
