# Introduction

Roca is a contractual language that compiles to JavaScript. It was born for AI-generated code -- the compiler enforces what humans can't review fast enough.

## Core idea

Every function has three sections: **logic**, **crash**, **test**. The body is pure happy path. Errors are handled in crash blocks. Proof tests are mandatory.

```roca
pub fn greet(name: String) -> String {
    const trimmed = name.trim()
    return "Hello " + trimmed
    crash { name.trim -> skip }
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
