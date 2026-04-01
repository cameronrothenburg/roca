# Roca

[![CI](https://github.com/cameronrothenburg/roca/actions/workflows/ci.yml/badge.svg)](https://github.com/cameronrothenburg/roca/actions/workflows/ci.yml)
[![npm](https://img.shields.io/npm/v/@rocalang/runtime)](https://www.npmjs.com/package/@rocalang/runtime)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![CodeRabbit Pull Request Reviews](https://img.shields.io/coderabbit/prs/github/cameronrothenburg/roca?utm_source=oss&utm_medium=github&utm_campaign=cameronrothenburg%2Froca&labelColor=171717&color=FF570A&link=https%3A%2F%2Fcoderabbit.ai&label=CodeRabbit+Reviews)](https://coderabbit.ai)

*Roca -- Spanish for rock, stone, cliff. Firme como una roca -- as solid as a rock.*

A contractual language that compiles to JavaScript and native machine code. A language for the AI era.

Our goal is simple: when you hear code was written in Roca, you trust it.

```bash
curl -sL https://raw.githubusercontent.com/cameronrothenburg/roca/master/install.sh | sh
roca init my-app && cd my-app
roca build
```

## The Problem

Programming languages were designed for humans to be expressive. That expressiveness gives AI too much freedom -- it can write code any way it wants, skip error handling, ignore edge cases, and return bare objects. Nothing in the language forces it to do better.

## The Solution

Roca is a narrow corridor. The compiler forces the AI (or human) to think about testing and error handling because it won't emit JavaScript until they do.

- **Built-in proof tests.** Every function has inline tests -- no output until they pass.
- **Every error is handled.** Crash blocks declare what happens when calls fail.
- **Function bodies are pure happy path.** No error variables, no if-err checks.
- **No null.** Use `-> Type, err` for failure cases, `Optional<T>` for absent fields.
- **Types are contracts.** The compiler validates every call, field, and return.

```roca
/// Validates an email address
pub fn validate(raw: String) -> Email, err {
    err missing = "email is required"
    err invalid = "email format is not valid"

    if raw == "" { return err.missing }
    if !raw.includes("@") { return err.invalid }
    return Email { value: raw }

    test {
        self("a@b.com") is Ok
        self("") is err.missing
        self("bad") is err.invalid
    }
}
```

The compiler checks the logic, runs the tests, and only then emits output. If something is wrong, no code is produced.

## Documentation

**[Language Specification](docs/src/spec/overview.md)** -- the definitive reference:

| Section | Description |
|---------|-------------|
| [1. Lexical Grammar](docs/src/spec/lexical.md) | Tokens, keywords, literals, operators |
| [2. Syntax](docs/src/spec/syntax.md) | Declarations, statements, expressions |
| [3. Type System](docs/src/spec/types.md) | Primitives, contracts, structs, enums, generics |
| [4. Module System](docs/src/spec/modules.md) | Imports, resolution, stdlib *(stub)* |
| [5. Error Model](docs/src/spec/errors.md) | Error returns, crash blocks, strategies |
| [6. Test Model](docs/src/spec/testing.md) | Test blocks, battle tests, auto-stubs |
| [7. Compilation](docs/src/spec/compilation.md) | JS emit, native emit, target differences |
| [8. Runtime](docs/src/spec/runtime.md) | Polyfills, memory model, concurrency |

**Quick start:** [Introduction](docs/src/introduction.md) | [Getting Started](docs/src/getting-started.md)

**Reference:** [Compiler Rules](docs/src/reference/compiler-rules.md) | [CLI](docs/src/reference/cli.md) | [Stdlib](docs/src/reference/stdlib.md)

## How It Works

1. You write `.roca` files with contracts, structs, and functions
2. `roca build` checks rules, compiles to JS, runs proof tests
3. Output: `.js` files + `.d.ts` TypeScript declarations
4. Your JS/TS project imports the compiled library

```bash
roca repl             # Interactive REPL (--native for JIT)
roca search trim      # Search stdlib and project symbols
roca skills           # Generate AI assistant skills
```

## License

Compiler: [AGPL-3.0](LICENSE-AGPL) -- modifications must stay open source.
Compiled output: [MIT](LICENSE-MIT) -- use your JS however you want.
