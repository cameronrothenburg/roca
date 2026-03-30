# Introduction

Roca — from the Spanish word for *rock* — is a contractual language that compiles to JavaScript.

Guaranteeing code is clean and error-free normally requires a stack of external tools — linters, scanners, test frameworks, type checkers — all configured and maintained separately. When an issue is found in one place, nothing forces fixing it everywhere else. AI makes this harder: it generates code fast, but without a feedback loop it has no reason to think about error handling, edge cases, or test coverage.

Roca is a narrow corridor. The compiler won't emit JavaScript until every error is handled, every function is tested, and every type is correct. When something is missing, the compiler error fills the AI's context with exactly what it needs — which error path is untested, which crash handler is absent, which type doesn't match. The feedback loop is the language itself.

## Core idea

Every function has three sections: **logic**, **crash**, **test**. The body is pure happy path. Errors are handled in crash blocks. Proof tests are mandatory.

```roca
/// Greets a person by name
pub fn greet(name: String) -> String {
    const trimmed = name.trim()
    return "Hello " + trimmed
    test {
        self("cam") == "Hello cam"
        self(" cam ") == "Hello cam"
    }
}
```

## What the compiler enforces

- Every function has a test block with at least one success case.
- Every error-returning call has a crash handler.
- Every error return path is tested.
- No null. No unhandled errors. No missing docs on public items.

These aren't lint warnings. They are hard errors. Code that violates them does not compile.

## Design principles

- **Happy path only** -- function bodies contain the success case. Crash blocks handle failure.
- **No null** -- use `Optional<T>` for absent fields, `-> Type, err` for fallible functions.
- **Explicit dependencies** -- extern contracts describe JS shapes and are passed as function parameters. No environment bags.
- **Proof tests are mandatory** -- every function ships with inline tests that run at build time.
- **Doc comments required** -- every `pub` item must have `///` or `/** */` documentation.

## Compilation target

Roca compiles to JavaScript. Error-returning functions use `{value, err}` objects. TypeScript `.d.ts` files are generated automatically with `RocaResult<T>` types. The output runs anywhere JS runs -- Node, Bun, Cloudflare Workers, browsers.
