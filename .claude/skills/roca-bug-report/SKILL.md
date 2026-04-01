---
name: roca-bug-report
description: Files structured bug reports when the AI encounters Roca compiler issues. Checks .bugreportagreed for consent before reporting. Appends to the same issue within a session.
---

# Roca Bug Report

Automatically file structured bug reports when you encounter Roca compiler bugs, unexpected behavior, or spec violations during development.

## Before reporting

1. Check if `.bugreportagreed` exists in the project root
2. If it does not exist, **do nothing** — silently skip
3. If it exists, read it as JSON and check `enabled` is `true`
4. If `enabled` is `false`, **do nothing** — silently skip

## When to invoke

Call this skill when you encounter:
- Compiler crashes, segfaults, or panics
- Proof test engine returning wrong values
- Parser rejecting valid syntax or accepting invalid syntax
- Checker producing false positives/negatives
- Emitter producing incorrect JS output
- Missing stdlib methods you expected to exist
- Behavior that contradicts `roca man` or `roca patterns`

## Collecting the report

Gather:
- **What happened**: the error output, stack trace, or unexpected behavior
- **What was expected**: reference the spec via `roca man` — cite the specific rule or section
- **Why it's wrong**: explain the mismatch between actual and expected
- **Minimal reproduction**: smallest `.roca` snippet that triggers the bug
- **Category**: Proof Test Engine, Parser, Checker, Emitter, Language Features, Stdlib, DX

Capture environment:
```bash
roca --version
uname -a
```

## Reading settings

Read `.bugreportagreed` for report style preferences:

```json
{
  "enabled": true,
  "agreed_at": "2026-04-01",
  "issue_number": null,
  "settings": {
    "include_sign_off_quote": true,
    "include_crab_rating": true,
    "include_ai_commentary": true,
    "tone": "snarky"
  }
}
```

- `include_sign_off_quote`: add an AI-generated closing quote about the experience
- `include_crab_rating`: add a crab emoji rating out of 5 with explanation
- `include_ai_commentary`: add inline editorial comments on each bug
- `tone`: style of commentary — `professional`, `snarky`, `deadpan`, `enthusiastic`

## Filing the issue

### First bug in a session (`issue_number` is null)

Create a new issue:

```bash
gh issue create --repo cameronrothenburg/roca \
  --title "Bug report: 1 issue found building [project name]" \
  --label "bug,bug report,automatic_review" \
  --body "[report body]"
```

Extract the issue number from the URL and write it back to `.bugreportagreed` in the `issue_number` field.

Auto-detect and add relevant labels: `parser`, `stdlib`, `extern`, `tests`, `native`, `emitter`, `checker`.

### Subsequent bugs in same session (`issue_number` is set)

Read the existing issue body, append the new bug under the appropriate category section, increment the title count, and update impact/summary sections:

```bash
gh issue view [number] --repo cameronrothenburg/roca --json body,title
gh issue edit [number] --repo cameronrothenburg/roca --title "[updated title]" --body "[updated body]"
```

## Report format

```markdown
# Automated Bug Report: [N] issues found building [project name]

> **Generated automatically by Claude Code ([model])** while [what you were doing].

**Environment:** Roca [version], [OS]
**Agent:** Claude Code ([model])

---

## [Category]

### [N]. [Bug title]

[Description of what happened]

\`\`\`roca
// minimal reproduction
\`\`\`

**Expected:** [what the spec says should happen, with reference]
**Actual:** [what actually happened]

[AI commentary if enabled]

---

## Impact

[Summary of how the bugs affected your work — what had to be worked around, what couldn't be written in Roca]

> [Sign-off quote if include_sign_off_quote is true]

**Overall rating:** [Crab rating if include_crab_rating is true]
```

## After filing

Tell the user you filed/updated a bug report and give them the issue URL.
