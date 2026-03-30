# Roca

*Roca — Spanish for rock, stone, cliff. Firme como una roca — as solid as a rock.*

A contractual language that compiles to JavaScript. A language for the AI era.

Our goal is simple: when you hear code was written in Roca, you trust it.

```bash
curl -sL https://raw.githubusercontent.com/cameronrothenburg/roca/master/install.sh | sh
roca init my-app && cd my-app
roca build
```

## The Problem

Programming languages were designed for humans to be expressive. That expressiveness gives AI too much freedom — it can write code any way it wants, skip error handling, ignore edge cases, and return bare objects. Nothing in the language forces it to do better.

Current tooling doesn't solve this. Linters, scanners, test frameworks, type checkers — all configured separately, all optional. When an issue is found in one place, nothing forces fixing it everywhere. AI generates code fast, but without a feedback loop it has no reason to think about error handling, edge cases, or test coverage.

## The Solution

Roca is a narrow corridor. The compiler forces the AI (or human) to think about testing and error handling because it won't emit JavaScript until they do. Every compiler error fills the AI's context with exactly what it needs — which error path is missing, which test case wasn't covered, which crash handler is absent. The feedback loop is the language itself.

This changes how you review code. The implementation has simple happy path logic and even simpler error handling — the compiler guarantees every error is handled and every path is tested. Bugs can still exist, but the surface area of unhandled, unsafe code shrinks dramatically. Review becomes: *is the contract verbose enough?* Does it cover the right error cases? Are the types precise?

- **Built-in unit testing.** Every function has inline proof tests — no JS emitted until they pass. Fuzz testing catches edge cases the developer missed. E2E testing is still your job — Roca guarantees the units are solid, you verify the feature works.
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

The compiler checks the logic, runs the tests, and only then emits JavaScript. If something is wrong, no output.

## Documentation

Full documentation in the [Roca Book](docs/src/SUMMARY.md):

**Philosophy** — [Safety by Compilation](docs/src/philosophy/safety.md) | [Happy Path](docs/src/philosophy/happy-path.md) | [No Null](docs/src/philosophy/no-null.md) | [Crash Blocks](docs/src/philosophy/crash-blocks.md)

**Syntax** — [Functions](docs/src/syntax/functions.md) | [Structs](docs/src/syntax/structs.md) | [Contracts](docs/src/syntax/contracts.md) | [Types](docs/src/syntax/types.md) | [Control Flow](docs/src/syntax/control-flow.md) | [Closures](docs/src/syntax/closures.md) | [Async](docs/src/syntax/async.md)

**Integration** — [Using in Projects](docs/src/integration/using-in-projects.md) | [Extern Contracts](docs/src/integration/extern-contracts.md) | [JS Wiring](docs/src/integration/js-wiring.md) | [TypeScript](docs/src/integration/typescript.md) | [Stdlib Modules](docs/src/integration/stdlib-modules.md)

**Reference** — [Compiler Rules](docs/src/reference/compiler-rules.md) | [CLI](docs/src/reference/cli.md) | [Stdlib](docs/src/reference/stdlib.md) | [Telemetry](docs/src/reference/telemetry.md)

**Project** — [Introduction](docs/src/introduction.md) | [Getting Started](docs/src/getting-started.md) | [Roadmap](ROADMAP.md)

Or use the CLI:

```bash
roca man              # Full language manual
roca patterns         # Coding patterns and JS integration
roca search trim      # Search stdlib and project
roca repl             # Interactive REPL
roca skills           # Generate AI assistant skills
```

## How It Works

1. You write `.roca` files with contracts, structs, and functions
2. `roca build` checks rules → compiles to JS → runs proof tests
3. Output: `.js` files + `.d.ts` TypeScript declarations
4. Your JS/TS project imports the compiled library

Roca functions that return errors use the `{value, err}` protocol:

```js
import { validate } from "my-roca-lib";

const { value: email, err } = validate("cam@test.com");
if (err) {
    console.error(err.name, err.message);
} else {
    console.log(email.value);
}
```

## License

Compiler: [AGPL-3.0](LICENSE-AGPL) — modifications must stay open source.
Compiled output: [MIT](LICENSE-MIT) — use your JS however you want.
