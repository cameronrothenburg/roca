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

### Step 2: Create ticket team

Create an agent team named `ticket-<number>` with a verifier and fixer teammate. The verifier works first, then hands off to the fixer with its findings.

```
Create an agent team named "ticket-<number>" to fix GitHub issue #<number>.
Spawn two teammates:

- "verifier" teammate using the roca-ticket-verify agent type:
  Verify that issue #<number> is still reproducible. If already fixed, close it.
  If reproducible, identify the affected crate and share findings with the fixer.

- "fixer" teammate using the roca-ticket-fix agent type:
  Wait for the verifier to confirm the issue is real.
  Fix the issue scoped to the identified crate.
  Read the crate-scoped skill for boundaries.
  Depends on: verifier completing.
```

The verifier can message the fixer directly with reproduction details, affected crate, and specific error output — giving the fixer richer context than a subagent handoff.

### Step 3: Monitor and intervene

Wait for the team to work through the issue:
- If the verifier can't reproduce (unclear steps), it messages the lead. Ask the user for guidance.
- If the verifier confirms it's already fixed, it closes the issue. Clean up the team and stop.
- If the fixer discovers the fix requires multiple crates, it messages the lead. Decide whether to expand scope or split into multiple tickets.

### Step 4: Stress test

After the fixer completes, spawn a `stress-tester` teammate to try to break the fix. It receives the issue context, the fix description, and the affected crate. Any failures must be fixed before proceeding.

### Step 5: Run tests

Run `/run-ci-local` to execute the full CI pipeline. This catches regressions across the entire workspace, not just the affected crate.

If tests fail, message the fixer with the failures.

### Step 6: Review

Clean up the ticket team, then run `/roca-review` to validate:
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
