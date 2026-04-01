---
name: roca-review
description: Full pre-PR review pipeline. Runs code-reviewer, spec-guardian, memory-tracker agents in parallel, then /coderabbit:review and /simplify. Must pass before creating a PR.
---

# Roca Review

Full review pipeline that must run before any pull request is created.

## Pipeline

Run these steps in order. If any step finds **blocking** issues, fix them before proceeding.

### Step 1: Run all agents in parallel

Launch these three agents concurrently using the Agent tool:

1. **code-reviewer** — cross-module consistency, Rust quality, Roca language rules
2. **spec-guardian** — verify changes match the language spec
3. **memory-tracker** — check native runtime for leaks, double frees, untracked allocations

Wait for all three to complete. Collect their reports.

### Step 2: Run /coderabbit:review

Run the CodeRabbit code review skill for general code quality, security, and best practices.

### Step 3: Run /simplify

Run the simplify skill to check for unnecessary complexity, duplication, and code quality issues in changed files.

### Step 4: Report

Compile a unified report:

```
## Roca Review Report

### Code Review
[summary from code-reviewer agent]

### Spec Compliance
[summary from spec-guardian agent]

### Memory Safety
[summary from memory-tracker agent]

### CodeRabbit
[summary from /coderabbit:review]

### Simplification
[summary from /simplify]

### Verdict: ✅ Ready for PR / ❌ Issues to fix
[list any blocking issues that must be resolved]
```

### Step 5: Fix or proceed

- If **all clear**: write the lock file `.claude/.review-passed` with content "passed" using Bash (`echo passed > .claude/.review-passed`), then ask the user with AskUserQuestion whether to proceed with creating the PR
- If **blocking issues found**: remove `.claude/.review-passed` if it exists (via Bash: `rm -f .claude/.review-passed`), list the issues, fix what you can automatically, and re-run the failing checks
- Do NOT create a PR until all blocking issues are resolved
