---
name: roca-review
description: "Full pre-PR review pipeline. TRIGGER when: user wants to review changes, prepare for a PR, or validate work (e.g. 'review this', 'is this ready for PR', 'check my changes'). Validates crate boundaries, KISS/SOLID/YAGNI, Rust quality, Roca language rules, memory safety, and runs tests."
---

# Roca Review

Full review pipeline that gates pull request creation. Validates everything from crate boundaries to test results.

## Pipeline

### Step 0: Identify changed crates

Run `git diff master...HEAD --name-only` to identify which files changed. Map each changed file to its crate. This determines which crate-scoped skills are relevant and which test suites to run.

### Step 1: Run tests

Run the test suites for all affected crates. If tests fail, stop here — fix failures before reviewing.

```bash
# Rust tests for affected crates
cargo test --release -p <crate-name>

# If any .roca files changed or emitter/checker changed:
cd tests/js && ROCA_BIN=../../target/release/roca bun test

# If CLI changed:
./target/release/roca check tests/js/projects/api
```

### Step 2: Run review agents in parallel

Launch these three agents concurrently:

1. **code-reviewer** — crate boundaries, KISS/SOLID/YAGNI, Rust quality, cross-crate consistency
2. **spec-guardian** — verify changes match the language spec
3. **memory-tracker** — check for leaks, double frees, untracked allocations (only if roca-cranelift, roca-native, or roca-runtime changed)

Wait for all to complete.

### Step 3: Run /coderabbit:review

General code quality, security, and best practices.

### Step 4: Run /simplify

Check changed files for unnecessary complexity and duplication.

### Step 5: Report and verdict

Compile a unified report:

```
## Roca Review Report

### Tests
[pass/fail summary, which suites ran]

### Crate Boundaries
[any boundary violations found by code-reviewer]

### KISS / SOLID / YAGNI
[any principle violations]

### Rust Quality
[type safety, unwrap usage, error handling]

### Spec Compliance
[summary from spec-guardian]

### Memory Safety
[summary from memory-tracker, or "N/A — no native changes"]

### CodeRabbit
[summary]

### Simplification
[summary]

### Verdict: PASS / FAIL
[list any blocking issues]
```

### Step 6: Verdict

- If **PASS**: tell the user the review passed and they can create the PR.
- If **FAIL**: list blocking issues. Fix what you can automatically, then re-run the failing checks. Do NOT tell the user it's ready until all blocking issues are resolved.
