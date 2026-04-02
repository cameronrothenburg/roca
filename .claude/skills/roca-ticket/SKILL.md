---
name: roca-ticket
description: "Full issue-to-PR pipeline. TRIGGER when: user references a GitHub issue to fix (e.g. 'fix issue 42', 'look at #87', 'handle that bug report'). Verifies the issue is reproducible, fixes it scoped to one crate, runs tests and review, then creates a PR."
---

# Roca Ticket

End-to-end pipeline: GitHub issue → verified fix → reviewed PR.

## Usage

```
/roca-ticket 42
/roca-ticket https://github.com/cameronrothenburg/roca/issues/42
```

## Pipeline

### Step 1: Load the issue

Fetch the issue details using `gh issue view <number>`. Extract:
- Title and description
- Labels (which crate? bug/feature/enhancement?)
- Any reproduction steps or error messages
- Linked PRs (already fixed?)

If the issue is closed, stop and tell the user.

### Step 2: Verify it's reproducible

Launch the **roca-ticket-verify** agent with the issue context. This agent:
- Attempts to reproduce the bug using the steps in the issue
- If already fixed on master, closes the issue with a comment and stops
- If reproducible, labels it as verified and continues

If verification fails (can't reproduce, unclear steps), ask the user for guidance before proceeding.

### Step 3: Identify the affected crate(s)

From the issue description, error messages, and reproduction:
- Map the problem to one or more crates
- Read the crate-scoped skill(s) for boundary context
- Identify the scope of the fix — single crate or cross-crate

### Step 4: Fix in a worktree

Launch the **roca-ticket-fix** agent. It:
- Works in an isolated git worktree
- Scopes the fix to the identified crate(s)
- Respects crate boundaries from the skills
- Adds or updates tests to cover the fix
- Creates commits following the conventional commit format

### Step 5: Run tests

Run the test suites for all affected crates:

```bash
cargo test --release -p <crate-name>
```

If emitter, checker, or .roca files were touched:
```bash
cd tests/js && ROCA_BIN=../../target/release/roca bun test
```

If tests fail, fix and re-run. Do not proceed until green.

### Step 6: Review

Run `/roca-review` to validate:
- Crate boundaries respected
- KISS/SOLID/YAGNI principles followed
- Rust quality checks pass
- Spec compliance (if language semantics changed)
- Memory safety (if native crates changed)

If review finds blocking issues, fix them and re-review.

### Step 7: Create PR

Create a pull request that:
- References the issue: `Fixes #<number>`
- Title follows conventional commit style
- Body includes what changed, why, and how it was tested
- Links the original issue for context

```bash
gh pr create --title "<type>(<scope>): <description>" --body "..."
```

Report the PR URL to the user.
