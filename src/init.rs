//! Project scaffolding for `roca init` and `roca skills`.

use std::fs;
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

/// Generate AI assistant skill files for the current project.
pub fn generate_skills(with_claude_md: bool) {
    write_file(
        Path::new(".claude/skills/roca-lang/SKILL.md"),
        include_str!("skills.md"),
    );
    println!("✓ .claude/skills/roca-lang/SKILL.md");

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
