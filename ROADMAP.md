# Roadmap

Track progress via [GitHub Issues](https://github.com/cameronrothenburg/roca/issues) and [Milestones](https://github.com/cameronrothenburg/roca/milestones).

## Current Release

**v0.2.0** — Working compiler with 778+ tests, used in production on one project.

Core features: parser, type checker, JS emitter, {value, err} protocol, .d.ts generation, proof tests + fuzz testing, crash blocks, extern contracts, std::json, LSP server, REPL.

## What's Next

Planned work is tracked as GitHub issues with milestone labels:

- [**v0.3.0**](https://github.com/cameronrothenburg/roca/milestone/1) — Stdlib expansion (std::http, std::crypto, std::time, std::url, std::encoding)
- [**v0.4.0**](https://github.com/cameronrothenburg/roca/milestone/2) — Language features (type aliases, destructuring, pipe operator)
- [**v0.5.0**](https://github.com/cameronrothenburg/roca/milestone/3) — Tooling (formatter, CI action, watch mode, VS Code publish)

## Contributing

The compiler is [AGPL-3.0](LICENSE-AGPL) — modifications must stay open source.
Compiled output is [MIT](LICENSE-MIT) — use your JS however you want.

To add a stdlib module, create `packages/stdlib/{name}.roca` + `packages/stdlib/{name}.js`. The compiler picks them up by filename.

See [issue templates](https://github.com/cameronrothenburg/roca/issues/new/choose) for bugs, features, and security reports.
