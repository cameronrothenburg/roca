# examples/worker

Built with [Roca](https://github.com/cameronrothenburg/roca) — a contractual language that compiles to JS.

## Commands

```bash
roca build              # compile all .roca files → JS (reads roca.toml)
roca check              # parse + check rules without emitting
roca test               # build + run proof tests, then clean output
roca run                # build + execute via bun
roca lsp                # start language server (stdio)
roca init <name>        # create a new project
```

All commands read `roca.toml` for `src=` and `out=` paths. Pass a file or directory to override.

## Language Rules

Read `.claude/skills/` for the full reference. Key rules:

1. Every function MUST have a `test` block — no exceptions
2. Every function call MUST have a `crash` handler
3. Types are `contract` (what), implementations are `struct` (how)
4. `satisfies` links a struct to a contract — one block per contract
5. Errors are named: `err name = "message"` in contracts, `err.name` in code
6. `pub` = exported, default = private
7. No `any`, `null`, `undefined` — every value has a provable type
8. If proof tests fail, no JS is emitted
9. `extern contract` / `extern fn` declare JS runtime types and functions
10. Generics use `<T>` with optional constraints: `<T: Loggable>`

@.claude/skills/roca-rules/SKILL.md
@.claude/skills/roca-contracts/SKILL.md
@.claude/skills/roca-patterns/SKILL.md
