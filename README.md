# Roca

A contractual language that compiles to JavaScript. Built for AI-generated code.

## The Problem

AI writes code well. It just doesn't write *safe* code. It skips validation, forgets error handling, returns bare objects, logs secrets. Not because it's dumb — because nothing stops it.

Code review catches these issues in human-written code. But when AI generates thousands of lines per day, review becomes a bottleneck. You need a compiler that enforces what humans can't review fast enough.

## The Solution

Roca is a language where safety is structural, not cultural.

- **Every function has proof tests.** No JS emitted until tests pass.
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

## Quick Start

```bash
cargo install --path .
roca init my-app && cd my-app
roca build
```

## Documentation

Full documentation is in the [Roca Book](docs/src/SUMMARY.md):

- [Introduction](docs/src/introduction.md) — what Roca is and why
- [Getting Started](docs/src/getting-started.md) — install, init, first build
- **Philosophy**
  - [Happy Path](docs/src/philosophy/happy-path.md) — function bodies are pure success
  - [No Null](docs/src/philosophy/no-null.md) — errors not null
  - [Crash Blocks](docs/src/philosophy/crash-blocks.md) — error handling
- **Syntax** — [Functions](docs/src/syntax/functions.md) | [Structs](docs/src/syntax/structs.md) | [Contracts](docs/src/syntax/contracts.md) | [Types](docs/src/syntax/types.md) | [Control Flow](docs/src/syntax/control-flow.md) | [Closures](docs/src/syntax/closures.md) | [Async](docs/src/syntax/async.md)
- **Integration** — [Extern Contracts](docs/src/integration/extern-contracts.md) | [JS Wiring](docs/src/integration/js-wiring.md) | [TypeScript](docs/src/integration/typescript.md) | [Stdlib Modules](docs/src/integration/stdlib-modules.md)
- **Reference** — [Compiler Rules](docs/src/reference/compiler-rules.md) | [CLI](docs/src/reference/cli.md) | [Stdlib](docs/src/reference/stdlib.md)

Or use the CLI:

```bash
roca man              # Full language manual
roca patterns         # Coding patterns and JS integration examples
roca search trim      # Search stdlib and project
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

MIT
