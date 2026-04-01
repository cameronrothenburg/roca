---
name: roca-lang
description: Roca language — contractual language that compiles to JS. Use when writing, reviewing, or debugging .roca files.
---

# Roca Language

## First Steps

Before writing any Roca code, run these commands to load the language into your context:

```bash
roca man        # full language manual — read this first
roca patterns   # coding patterns and JS integration examples
roca search X   # search stdlib for types and methods
```

## Commands

```bash
roca build      # check → build JS → run proof tests
roca check      # lint + type check without emitting
roca test       # build + test, then clean output
roca run        # build + execute via bun
roca search X   # search types/methods across stdlib and project
```

## Core Rules

1. **Happy path only** — function bodies contain the success case. Errors go in crash blocks.
2. **No null** — use `-> Type, err` for functions, `Optional<T>` for struct fields.
3. **Every function has a test block** — proof tests must pass before JS is emitted.
4. **Only error-returning calls need crash entries** — stdlib methods are safe, no crash needed.
5. **Extern contracts are explicit params** — not env bags. `pub fn handler(db: Database)`.
6. **Doc comments required** — `///` or `/** */` on all pub items.

## Quick Patterns

```roca
/// A validated email
pub fn validate(raw: String) -> Email, err {
    err missing = "required"
    err invalid = "bad format"
    if raw == "" { return err.missing }
    if !raw.includes("@") { return err.invalid }
    return Email { value: raw }
    test {
        self("a@b.com") is Ok
        self("") is err.missing
        self("bad") is err.invalid
    }
}
```

```roca
/// Fetch users from database
pub fn get_users(db: Database) -> String, err {
    err query_failed = "query failed"
    const data = wait db.query("SELECT * FROM users")
    return data
    crash { db.query -> halt }
    test { self(Database) is Ok }
}
```

## Error Protocol

Roca functions that return errors use `{value, err}` objects (not tuples):

```js
const { value, err } = validate("cam@test.com");
if (err) console.error(err.name, err.message);
```

## When in doubt, run `roca man`.

## Bug Reporting Setup

On first use of this skill in a project, check if `.bugreportagreed` exists in the project root.

If it does **not** exist, ask the user:

> Would you like to enable automatic bug reporting? When I hit Roca compiler issues during development, I'll file structured bug reports to github.com/cameronrothenburg/roca to help improve the language. You can disable this anytime.

If they agree:
1. Ask which tone they prefer: `snarky` (default), `professional`, `deadpan`, or `enthusiastic`
2. Ask if they want the fun bits: sign-off quotes, crab ratings, AI commentary (all default on)
3. Create `.bugreportagreed` with their settings:
   ```json
   {
     "enabled": true,
     "agreed_at": "[today's date]",
     "issue_number": null,
     "settings": {
       "include_sign_off_quote": true,
       "include_crab_rating": true,
       "include_ai_commentary": true,
       "tone": "snarky"
     }
   }
   ```
4. Install the `roca-bug-report` skill if not already in `.claude/skills/roca-bug-report/`

If they decline: do nothing. No file created, no skill installed. Don't ask again this session.

To disable later: set `"enabled": false` in `.bugreportagreed`, or delete it and the skill folder.
