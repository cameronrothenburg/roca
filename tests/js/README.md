# JS Tests

Two layers of testing for the JavaScript target:

## Layer 1: Runtime Tests (`runtime.test.js`)

Tests `@rocalang/runtime` directly — does each stdlib contract work?

## Layer 2: Compiler Output Tests (`compiler.test.js`)

Tests that `roca build` produces correct JS — does the emitter output valid code that runs with the runtime?

## Running

```bash
cd tests/js
bun install
bun test
```

Uses the local `packages/runtime/` via `file:` dependency.
