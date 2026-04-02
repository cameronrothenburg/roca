---
name: roca-feature
description: "Spec-driven feature development pipeline. TRIGGER when: user asks to add a new language feature, implement a spec section, or build a new construct (e.g. 'add pattern matching', 'implement guards', 'build the enum spread syntax'). Takes an idea, spec reference, or issue number. Generates failing tests first, then implements across crates via an agent team."
---

# Roca Feature

Spec-driven, test-first feature development across the Roca compiler workspace using agent teams.

## Usage

```
/roca-feature pattern matching with guards
/roca-feature docs/src/spec/syntax.md#2.8
/roca-feature 87
```

## Pipeline

### Phase 0: Input & Spec Resolution

1. Parse the input:
   - **GitHub issue number or URL**: fetch with `gh issue view <number>`, extract feature description
   - **Spec section reference**: read that section directly
   - **Prose description**: use as the feature idea

2. Search `docs/src/spec/` for existing spec coverage (grep for keywords from the feature).

3. Branch:
   - **Spec exists** → extract grammar, semantics, error cases → skip to Phase 2
   - **No spec** → proceed to Phase 1

### Phase 1: Spec Drafting (only if no spec exists)

1. Load language context:
   ```bash
   roca man
   roca patterns
   ```
2. Read adjacent spec sections in `docs/src/spec/` for formatting conventions (RFC 2119 keywords, grammar notation).
3. Read `docs/src/reference/compiler-rules.md` for error code format.
4. Draft the spec section with these subsections:
   - **Syntax** — grammar production rules
   - **Semantics** — what the construct means, MUST/SHOULD/MAY rules
   - **Error cases** — which checker rules apply, new error codes needed
   - **Compilation** — how it maps to JS output and native JIT
   - **Examples** — valid and invalid Roca source

5. **Present to the user for approval.** Do NOT proceed until the user confirms the spec. This is a hard gate.

6. Write the approved spec to `docs/src/spec/`.

### Phase 2: Crate Impact Analysis

1. Map spec requirements to affected crates:

   | Spec Element | Crate(s) |
   |---|---|
   | New syntax / grammar | roca-ast (nodes), roca-parse (tokenizer + parser) |
   | New keywords | roca-ast (constants), roca-parse (tokenizer) |
   | New type constructs | roca-types (RocaType variant), roca-ast (TypeRef variant) |
   | New error codes | roca-errors (constants) |
   | New checker rules | roca-check (rule file + registration) |
   | JS output behavior | roca-js (emitter) |
   | Native JIT behavior | roca-native (AST→Body translation) |
   | New Body API methods | roca-cranelift (only if needed) |
   | New stdlib functions | roca-runtime + packages/stdlib/ |

2. Build the implementation wave order:
   ```
   Wave 0 (parallel): roca-ast, roca-errors, roca-types
   Wave 1 (parallel): roca-parse, roca-check
   Wave 2 (parallel): roca-js, roca-native [, roca-cranelift]
   ```

3. Read each affected crate's skill (`.claude/skills/roca-*-crate/SKILL.md`) for boundaries, key files, test patterns, and YAGNI rules.

### Phase 3: Test Generation (TDD — before any implementation)

Generate failing tests from the spec. Tests MUST be written and confirmed failing before any implementation begins.

#### Parser tests

In `crates/roca-parse/src/parser_tests.rs`:

```rust
#[test]
fn parse_<feature>() {
    let file = parse(r#"<valid roca source>"#);
    assert_eq!(file.items.len(), N);
    // assert on the new AST node structure
}

#[test]
#[should_panic]
fn reject_<invalid_case>() {
    parse(r#"<invalid source>"#);
}
```

#### Checker tests

In `crates/roca-check/src/lib.rs` inside `check_tests`:

```rust
#[test]
fn <feature>_valid_program() {
    let file = parse::parse(r#"<valid source>"#);
    let errors = check(&file);
    assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
}

#[test]
fn <feature>_rejects_<violation>() {
    let file = parse::parse(r#"<source violating rule>"#);
    let errors = check(&file);
    assert!(errors.iter().any(|e| e.code == "<ERROR_CODE>"));
}
```

One test per error case from the spec, plus at least one valid-program test.

#### Native/JIT tests

In `crates/roca-native/src/tests_features.rs`:

```rust
#[test]
fn <feature>_returns_correct_value() {
    let mut m = jit(r#"pub fn test_fn() -> <Type> { <source> }"#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "test_fn")) };
    assert_eq!(f(), <expected>);
}
```

#### JS end-to-end tests

In `tests/js/verify.test.js` inside a new `describe("<feature>")` block:

```javascript
describe("<feature>", () => {
    test("<case>", () => {
        expect(run(
            `<roca source>`,
            `console.log(<test expression>);`,
        )).toBe("<expected>");
    });
});
```

#### Confirm all tests fail

```bash
cargo test --release -p roca-parse -- <feature>
cargo test --release -p roca-check -- <feature>
cargo test --release -p roca-native -- <feature>
cd tests/js && ROCA_BIN=../../target/release/roca bun test verify.test.js
```

Report the failure count. If any pass, the feature is already partially implemented — adjust scope.

### Phase 4: Create Agent Team for Implementation

Create an agent team to implement the feature across crates. Teammates use the `roca-feature-crate` agent type and can communicate cross-crate issues to each other directly.

#### Team structure

Create a team named `feature-<short-name>` with one teammate per affected crate. Each teammate receives:
- The feature spec section
- Its crate name and scoped skill path
- The specific changes needed
- The test names it must make pass

```
Create an agent team named "feature-<name>" to implement <feature> across the compiler.
Spawn teammates using the roca-feature-crate agent type:

- "ast" teammate: add new node types to roca-ast. Changes: <list>. Tests to pass: <list>.
  Read .claude/skills/roca-foundation-crate/SKILL.md for boundaries.

- "parse" teammate: add parser support in roca-parse. Changes: <list>. Tests to pass: <list>.
  Read .claude/skills/roca-parse-crate/SKILL.md for boundaries.
  Depends on: ast teammate completing.

- "check" teammate: add checker rules in roca-check. Changes: <list>. Tests to pass: <list>.
  Read .claude/skills/roca-check-crate/SKILL.md for boundaries.
  Depends on: ast teammate completing.

- "js" teammate: add JS emission in roca-js. Changes: <list>. Tests to pass: <list>.
  Read .claude/skills/roca-js-crate/SKILL.md for boundaries.
  Depends on: parse teammate completing.

- "native" teammate: add native compilation in roca-native. Changes: <list>. Tests to pass: <list>.
  Read .claude/skills/roca-native-crate/SKILL.md for boundaries.
  Depends on: parse teammate completing.

Require plan approval before teammates make changes.
```

Only spawn teammates for crates that are actually affected. Skip crates that don't need changes.

#### Task dependencies enforce wave order

Use task dependencies so the team self-coordinates:
- Wave 0 tasks (ast, errors, types) have no dependencies — teammates start immediately
- Wave 1 tasks (parse, check) depend on Wave 0 tasks completing
- Wave 2 tasks (js, native) depend on Wave 1 tasks completing

Teammates will automatically pick up work as their dependencies resolve.

#### Cross-crate communication

Unlike subagents, teammates can message each other when they discover cross-crate issues:
- Native teammate discovers a missing Body API method → messages the cranelift teammate
- Parse teammate adds a new AST variant → broadcasts to check, js, and native teammates
- Check teammate finds a new error case → messages the spec lead

This is the key advantage over subagents — crate agents collaborate instead of working in isolation.

#### Verification between waves

After each wave completes:

**Wave 0:**
```bash
cargo build --release
```
Expect downstream match-arm errors — confirms AST changes rippled correctly.

**Wave 1:**
```bash
cargo test --release -p roca-parse -- <feature>
cargo test --release -p roca-check -- <feature>
```

**Wave 2:**
```bash
cargo test --release
cd tests/js && ROCA_BIN=../../target/release/roca bun test
```

If tests fail after a wave, message the responsible teammate with the error output.

### Phase 5: Stress Test

Spawn a `stress-tester` teammate to actively try to break the new feature. It receives:
- What was just built (the feature spec + which crates changed)
- The affected crate names and file paths

The stress tester writes adversarial tests targeting boundary values, ownership edge cases, error paths, and feature combinations. Any failures it finds must be fixed before proceeding.

### Phase 6: Integration Verification

Run `/run-ci-local` to execute the full CI pipeline (build, workspace tests, smoke test, JS integration tests). This mirrors `.github/workflows/ci.yml` exactly.

Confirm all originally-failing tests from Phase 3 now pass, plus any new stress tests. Report any remaining failures.

### Phase 7: Documentation

1. **Compiler rules** (`docs/src/reference/compiler-rules.md`) — add any new error codes
2. **Manual** (`src/manual.txt`) — add the new construct with syntax and examples
3. **Patterns** (`src/patterns.txt`) — add a pattern if the feature introduces new coding idioms
4. **Integration test** (`tests/integration/`) — add a `.roca` file demonstrating the feature in a realistic scenario

### Phase 8: Clean Up and Review

1. Clean up the agent team.
2. Run `/roca-review`. If blocking issues are found, fix them and re-review until PASS.
