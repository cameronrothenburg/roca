# Roca Language Specification

**Version:** 0.3.0-draft
**Status:** Working Draft
**Authors:** Cameron Rothenburg

## Abstract

Roca is a contractual programming language that compiles to JavaScript and native machine code. Every function has proof tests. Every error is handled by crash blocks. Function bodies are pure happy path.

This specification defines the lexical grammar, syntax, type system, module system, error model, test model, compilation targets, and runtime requirements for conforming Roca implementations.

## Conventions

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

## Document Structure

| Section | Status | Description |
|---------|--------|-------------|
| [1. Lexical Grammar](./lexical.md) | Draft | Tokens, keywords, literals, operators |
| [2. Syntax](./syntax.md) | Draft | Declarations, statements, expressions |
| [3. Type System](./types.md) | Draft | Primitives, contracts, structs, enums, generics |
| [4. Module System](./modules.md) | Stub | Imports, resolution, stdlib, extern contracts |
| [5. Error Model](./errors.md) | Draft | Error returns, crash blocks, strategies |
| [6. Test Model](./testing.md) | Draft | Test blocks, battle tests, auto-stubs |
| [7. Compilation](./compilation.md) | Draft | JS emit, native emit, target differences |
| [8. Runtime](./runtime.md) | Draft | Polyfills, bridges, memory model |

## Design Principles

1. **Happy path only.** Function bodies contain the success case. Errors are handled in crash blocks. Mutation methods MAY use `let val, err = call()` inline when branching on errors is required.
2. **Prove it works.** Every public function MUST have a test block with concrete input/output assertions.
3. **Contracts, not classes.** Types are defined by what they can do (contracts), not what they are (inheritance).
4. **Compile-time safety.** The compiler enforces error handling, type safety, and test coverage before any code runs.
5. **Target independence.** The same Roca source MUST produce equivalent behavior on all compilation targets.
