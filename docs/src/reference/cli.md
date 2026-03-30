# CLI

## Commands

| Command | Description |
|---------|-------------|
| `roca init <name>` | Create a new project with `roca.toml` and `src/` |
| `roca check [path]` | Parse + lint + type check |
| `roca build [path]` | Check, build JS, run proof tests |
| `roca test [path]` | Build + test, then clean output |
| `roca run [path]` | Build + execute via embedded V8 |
| `roca lsp` | Start the language server |
| `roca man` | Show the language manual |
| `roca --version` | Print version |

All commands read `roca.toml` for configuration when no path is given.

## Configuration

`roca.toml` at the project root:

```toml
[project]
name = "my-app"
version = "0.1.0"

[build]
src = "src/"
out = "out/"
mode = "jslib"         # optional: produces package.json, runs npm install
tracking = false       # optional: disable compilation logs
```

## Build modes

**Default** -- compiles `.roca` files to `.js` in the `out/` directory.

**jslib** -- additionally produces `out/package.json` and runs `npm install`, so your JS project can import the output as a dependency.

## Observability

All compilation events are logged to `~/.roca/logs/roca.jsonl`:

| Event | Fields |
|-------|--------|
| `parse_error` | file, message, source code |
| `check_errors` | file, errors with code/message/context, source |
| `test_result` | file, passed/failed count, output |
| `build_success` | file, output path |
| `build_failed` | file, reason |

Disable with `tracking = false` in `roca.toml`.
