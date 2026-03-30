# Getting Started

## Prerequisites

- [Rust](https://rustup.rs/) — to build the compiler from source

Install the compiler:

```bash
cargo install --path .
```

Or grab a prebuilt binary from the [releases page](https://github.com/cameronrothenburg/roca/releases).

## Create a project

```bash
roca init my-app
```

This creates a directory with a `roca.toml` configuration file and a `src/` directory.

## Configuration

Projects are configured with `roca.toml`:

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

## Write your first file

Create `src/greet.roca`:

```roca
/// Greets a user by name
pub fn greet(name: String) -> String {
    return "Hello " + name.trim()
    test {
        self("cam") == "Hello cam"
        self(" cam ") == "Hello cam"
    }
}
```

## Commands

| Command | Description |
|---------|-------------|
| `roca check [path]` | Parse, lint, and type check |
| `roca build [path]` | Check, build JS, run proof tests |
| `roca test [path]` | Build + test, then clean output |
| `roca run [path]` | Build + execute via embedded V8 |
| `roca lsp` | Start the language server |
| `roca man` | Show the language manual |
| `roca --version` | Print version |

All commands read `roca.toml` when no path is given.

## Build and test

```bash
roca build
```

This runs check, compiles to JS in `out/`, and executes all proof tests. If any test fails, the build fails.

## jslib mode

When `mode = "jslib"` is set in `roca.toml`, `roca build` produces `out/package.json` and runs `npm install`. Your JS project can then import the compiled output:

```js
import { greet } from "my-app";
```
