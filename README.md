# Roca

[![CI](https://github.com/cameronrothenburg/roca/actions/workflows/ci.yml/badge.svg)](https://github.com/cameronrothenburg/roca/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)

A memory-safe language built for AI. Compiles to JavaScript or native binary. The compiler enforces correctness through explicit ownership, mandatory proof tests, and error messages that teach you the fix.

```bash
cargo install rocalang
roca init my-app && cd my-app
roca build
```

## Why

AI writes code any way it wants. Nothing in most languages forces it to handle errors, manage memory, or prove correctness. Roca does.

The compiler is the feedback loop. Write code, get a teaching error, fix it, prove it works. Every cycle makes the next attempt correct.

## What It Does

- **Ownership without annotations.** `const` owns, `let` borrows, `o`/`b` on parameters. The compiler infers the rest. If it can't infer, it's a compile error — not a runtime crash.
- **Proof tests built in.** Every function has inline tests. No output until they pass.
- **Every error handled.** `let val, err = call()` — you check it or you don't compile.
- **Dual output.** Same source compiles to JS (GC handles memory) or native binary (ownership handles memory).
- **Errors that teach.** Every diagnostic shows what you wrote, why it's wrong, and what to write instead.

```roca
pub fn validate(b raw: String) -> Email, err {
    err missing = "email is required"
    err invalid = "email format is not valid"

    if raw == "" { return err.missing }
    if !raw.includes("@") { return err.invalid }

    const email = Email { value: raw }
    return email

test {
    self("a@b.com") is Ok
    self("") is err.missing
    self("bad") is err.invalid
}}
```

The compiler checks the logic, runs the tests natively, and only then emits output. If something is wrong, no code is produced.

## The Feedback Loop

```
Write .roca → Compiler infers ownership → Native proves it compiles
    → Proof tests verify correctness → JS or binary emitted
         ↑                                          │
         └──── teaching error shows the fix ←───────┘
```

## Documentation

| Section | Description |
|---------|-------------|
| [Syntax](docs/src/spec/syntax.md) | Grammar, `o`/`b` parameters, statements, expressions |
| [Memory Model](docs/src/spec/memory.md) | Ownership rules, second-class references, last-use destruction |
| [Error Codes](docs/src/spec/errors.md) | All compiler diagnostics by domain |
| [AI Feedback Loop](docs/src/spec/feedback.md) | Teaching error messages — what you wrote, why, what instead |

## License

Compiler: [AGPL-3.0](LICENSE-AGPL) — modifications must stay open source.
Compiled output: [MIT](LICENSE-MIT) — use your JS however you want.
