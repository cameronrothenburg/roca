//! Project scaffolding for `roca init` and `roca skills`.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("error creating {}: {}", parent.display(), e);
            std::process::exit(1);
        });
    }
    fs::write(path, content).unwrap_or_else(|e| {
        eprintln!("error writing {}: {}", path.display(), e);
        std::process::exit(1);
    });
}

/// Create a new Roca project with roca.toml, src/, and a starter file.
pub fn init_project(name: &str) {
    let root = Path::new(name);

    if root.exists() {
        eprintln!("error: '{}' already exists", name);
        std::process::exit(1);
    }

    write_file(&root.join("roca.toml"), &format!(
r#"[project]
name = "{name}"
version = "0.1.0"

[build]
src = "src/"
out = "out/"
"#));

    write_file(&root.join(".gitignore"), "out/\nnode_modules/\n*.test.js\n");

    write_file(&root.join("src").join("main.roca"), &format!(
r#"/// {name} entry point
pub fn hello(name: String) -> String {{
    return "Hello from " + name

    test {{
        self("Roca") == "Hello from Roca"
        self("") == "Hello from "
    }}
}}
"#));

    println!("✓ created {}", name);
    println!("  cd {} && roca build", name);
    println!();
    println!("  roca skills          generate AI assistant skills");
    println!("  roca skills --claude  also generate CLAUDE.md");
}

fn prompt_input(question: &str, default: &str) -> String {
    print!("{} [{}]: ", question, default);
    let _ = io::stdout().flush();
    let mut input = String::new();
    if io::stdin().lock().read_line(&mut input).is_err() {
        return default.to_string();
    }
    let trimmed = input.trim();
    if trimmed.is_empty() { default.to_string() } else { trimmed.to_string() }
}

fn prompt_yn(question: &str, default: bool) -> bool {
    let d = if default { "Y/n" } else { "y/N" };
    let answer = prompt_input(question, d);
    match answer.to_lowercase().as_str() {
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default,
    }
}

/// Generate AI assistant skill files for the current project.
pub fn generate_skills(with_claude_md: bool) {
    write_file(
        Path::new(".claude/skills/roca-lang/SKILL.md"),
        include_str!("skills.md"),
    );
    println!("✓ .claude/skills/roca-lang/SKILL.md");

    println!();
    println!("Roca can automatically file bug reports when the AI hits");
    println!("compiler issues during development. Reports go to:");
    println!("  github.com/cameronrothenburg/roca/issues");
    println!();
    if prompt_yn("Enable automatic bug reporting?", true) {
        let tone = loop {
            let t = prompt_input(
                "Report tone? (snarky/professional/deadpan/enthusiastic)",
                "snarky",
            );
            match t.as_str() {
                "snarky" | "professional" | "deadpan" | "enthusiastic" => break t,
                _ => println!("  Valid tones: snarky, professional, deadpan, enthusiastic"),
            }
        };
        let sign_off = prompt_yn("Include sign-off quotes?", true);
        let emoji_rating = prompt_yn("Include emoji rating?", true);
        let commentary = prompt_yn("Include AI commentary?", true);

        let enabled = |b: bool| if b { "enabled" } else { "disabled" };

        write_file(
            Path::new(".claude/skills/roca-bug-report/SKILL.md"),
            &format!(
r#"---
name: roca-bug-report
description: Files structured bug reports when the AI encounters Roca compiler issues. Appends to the same issue within a session.
---

# Roca Bug Report

Automatically file structured bug reports when you encounter Roca compiler bugs,
unexpected behavior, or spec violations during development.

## When to invoke

Call this skill when you encounter:
- Compiler crashes, segfaults, or panics
- Proof test engine returning wrong values
- Parser rejecting valid syntax or accepting invalid syntax
- Checker producing false positives/negatives
- Emitter producing incorrect JS output
- Missing stdlib methods you expected to exist
- Behavior that contradicts `roca man` or `roca patterns`

## Report Settings

- **Tone:** {tone}
- **Sign-off quotes:** {sign_off}
- **Emoji rating:** {emoji_rating}
- **AI commentary:** {commentary}

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

## Before filing

Search for existing open issues that match the bug you found:

```bash
gh issue list --repo cameronrothenburg/roca --state open --search "[keywords from the bug]"
```

- If a **matching issue exists**: add a comment with your reproduction case and environment info instead of creating a new issue. This groups similar reports together.
- If **no match**: create a new issue.

## Filing a new issue

Each bug gets its **own issue**. One bug, one ticket — easier to triage, label, assign, and close.

```bash
gh issue create --repo cameronrothenburg/roca \
  --title "[Category]: [short bug description]" \
  --label "triage,ai-generated" \
  --body "[report body]"
```

## Report format

```markdown
> **Generated automatically by Claude Code** while [what you were doing].

**Environment:** Roca [version], [OS]
**Category:** [Proof Test Engine / Parser / Checker / Emitter / Language Features / Stdlib / DX]

## What happened

[Description of the bug with error output]

\`\`\`roca
// minimal reproduction
\`\`\`

## Expected

[What the spec says should happen — cite the specific rule or section from roca man]

## Actual

[What actually happened]

[AI commentary — match the tone setting above]

> [Sign-off quote — only if sign-off quotes are enabled above]

**Rating:** [Pick an emoji that fits the vibe, rate out of 5 — only if emoji rating is enabled above]
```

## After filing

Tell the user you filed the bug report and give them the issue URL.
"#,
                tone = tone,
                sign_off = enabled(sign_off),
                emoji_rating = enabled(emoji_rating),
                commentary = enabled(commentary),
            ),
        );
        println!("✓ .claude/skills/roca-bug-report/SKILL.md");
    } else {
        println!("  Skipped bug reporting.");
    }

    if with_claude_md {
        let name = fs::read_to_string("roca.toml")
            .ok()
            .and_then(|c| c.lines()
                .find(|l| l.starts_with("name"))
                .and_then(|l| l.split('"').nth(1))
                .map(|s| s.to_string()))
            .unwrap_or_else(|| "roca-project".to_string());

        write_file(Path::new("CLAUDE.md"), &format!(
r#"# {name}

Built with [Roca](https://github.com/cameronrothenburg/roca) — a contractual language that compiles to JS.

## Before writing Roca code, run:

```bash
roca man       # full language manual
roca patterns  # coding patterns and JS integration
roca search X  # search stdlib for types/methods
```

## Commands

```bash
roca build     # check → build JS → proof tests
roca check     # lint + type check only
roca test      # build + test, clean output
roca run       # build + execute via bun
```

@.claude/skills/roca-lang/SKILL.md
"#));
        println!("✓ CLAUDE.md");
    }

    println!();
    println!("Skills installed. AI assistants will use `roca man`,");
    println!("`roca patterns`, and `roca search` to write Roca code.");
}
