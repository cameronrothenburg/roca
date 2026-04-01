---
name: gen-test
description: Generate Roca test files with inline proof tests, crash blocks, and proper error handling. Use when user asks to create test files or scaffold new .roca modules with tests.
disable-model-invocation: true
---

# Generate Roca Test File

## Before generating

1. Run `roca man` and `roca patterns` to load the language spec
2. Check existing tests in `tests/integration/` for style reference
3. Check `packages/stdlib/` for available stdlib contracts

## Rules

- Every function MUST have an inline `test {}` block — this is not optional
- Error-returning functions need `crash {}` blocks for every fallible call
- Use `self()` to call the function under test inside test blocks
- Use `is Ok` and `is err.name` for assertions
- Doc comments (`///`) required on all `pub` items
- No null — use `-> Type, err` for failures, `Optional<T>` for absent values
- Happy path only in function bodies — errors go in crash blocks

## Template

```roca
import { Dependency } from std::module

/// Description of what this function does
pub fn function_name(input: String) -> ResultType, err {
    err invalid_input = "input validation failed"
    err operation_failed = "the operation failed"

    const result = wait Dependency.method(input)
    return result

    crash {
        Dependency.method -> fallback(default_value)
    }

    test {
        self("valid") is Ok
        self("") is err.invalid_input
    }
}
```

## For structs with contracts

```roca
pub contract ServiceContract {
    process(input: String) -> String, err {
        err failed = "processing failed"
    }
}

pub struct Service {
    process(input: String) -> String, err {
        err failed = "processing failed"
    }
}{
    /// Process the input
    pub fn process(input: String) -> String, err {
        const data = input.trim()
        if data == "" { return err.failed }
        return data

        test {
            self("hello") is Ok
            self("") is err.failed
        }
    }
}

Service satisfies ServiceContract
```

## After generating

```bash
roca check path/to/file.roca   # validate without emitting
roca build path/to/file.roca   # full build with proof tests
```
