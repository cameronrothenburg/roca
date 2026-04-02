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

### Step 2: Create review team

Create an agent team named `review-<branch>` with three reviewer teammates. Each reviewer works independently but can challenge each other's findings directly.

```
Create an agent team named "review-<branch>" to review changes on this branch.
Spawn three teammates:

- "boundaries" teammate using the code-reviewer agent type:
  Focus on crate boundary violations, KISS/SOLID/YAGNI, Rust quality, and cross-crate consistency.

- "spec" teammate using the spec-guardian agent type:
  Verify changes match the language spec. Check for breaking changes.

- "memory" teammate using the memory-tracker agent type:
  Check for leaks, double frees, untracked allocations, ABI mismatches.
  (Only spawn if roca-cranelift, roca-native, or roca-runtime changed.)

Have them review in parallel, then share and challenge each other's findings
before reporting a final verdict.
```

The reviewers can message each other to cross-reference findings — e.g., the spec reviewer finds a semantic change and asks the memory reviewer if it affects allocation patterns. This produces better reviews than isolated subagents.

Wait for all to complete and synthesize their reports.

### Step 3: Run /coderabbit:review

General code quality, security, and best practices.

### Step 4: Run /simplify

Check changed files for unnecessary complexity and duplication.

### Step 5: Report and verdict

Clean up the review team, then compile a unified report:

```
## Roca Review Report

### Tests
[pass/fail summary, which suites ran]

### Crate Boundaries
[any boundary violations found by boundaries reviewer]

### KISS / SOLID / YAGNI
[any principle violations]

### Rust Quality
[type safety, unwrap usage, error handling]

### Spec Compliance
[summary from spec reviewer]

### Memory Safety
[summary from memory reviewer, or "N/A — no native changes"]

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
