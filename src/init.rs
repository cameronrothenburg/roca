use std::fs;
use std::path::Path;

pub fn init_project(name: &str) {
    let root = Path::new(name);

    if root.exists() {
        eprintln!("error: '{}' already exists", name);
        std::process::exit(1);
    }

    let write = |path: &Path, content: &str| {
        fs::write(path, content).unwrap_or_else(|e| {
            eprintln!("error writing {}: {}", path.display(), e);
            std::process::exit(1);
        });
    };

    fs::create_dir_all(root.join("src")).unwrap_or_else(|e| {
        eprintln!("error creating {}: {}", root.join("src").display(), e);
        std::process::exit(1);
    });
    fs::create_dir_all(root.join(".claude").join("skills")).unwrap_or_else(|e| {
        eprintln!("error creating directories: {}", e);
        std::process::exit(1);
    });

    write(&root.join("roca.toml"), &format!(r#"[project]
name = "{name}"
version = "0.1.0"

[build]
src = "src/"
out = "out/"
"#));

    write(&root.join(".gitignore"), "out/\nnode_modules/\n*.test.js\n");

    write(&root.join("src").join("main.roca"), &r#"// {name} — built with Roca

contract Stringable {
    to_string() -> String
}

pub struct App {
    name: String

    create(name: String) -> App
}{
    fn create(name: String) -> App {
        return App { name: name }

        test {}
    }
}

App satisfies Stringable {
    fn to_string() -> String {
        return self.name

        test {}
    }
}

pub fn hello(name: String) -> String {
    return "Hello from " + name

    test {
        self("Roca") == "Hello from Roca"
        self("") == "Hello from "
    }
}
"#.replace("{name}", name));

    write(&root.join("CLAUDE.md"), &format!(r#"# {name}

Built with [Roca](https://github.com/cameronrothenburg/roca) — a contractual language that compiles to JS.

## Build

```bash
roca build src/       # compile all .roca files → JS
roca check src/       # check rules without emitting
roca run src/main.roca  # build + execute
```

## Language Rules

Read `.claude/skills/` for the full reference. Key rules:

1. Every function MUST have a `test` block — no exceptions
2. Every function call MUST have a `crash` handler
3. Types are `contract` (what), implementations are `struct` (how)
4. `satisfies` links a struct to a contract — one block per contract, never chained
5. Errors are named: `err name = "message"` in contracts, `err.name` in code
6. `pub` = exported, default = private
7. No `any`, `null`, `undefined` — every value has a provable type
8. If proof tests fail, no JS is emitted

@.claude/skills/roca-rules.md
@.claude/skills/roca-contracts.md
@.claude/skills/roca-patterns.md
"#, name = name));

    write(&root.join(".claude").join("skills").join("roca-rules.md"), SKILL_RULES);
    write(&root.join(".claude").join("skills").join("roca-contracts.md"), SKILL_CONTRACTS);
    write(&root.join(".claude").join("skills").join("roca-patterns.md"), SKILL_PATTERNS);

    println!("✓ created {}", name);
    println!("  cd {} && roca build src/", name);
}

const SKILL_RULES: &str = r#"# Roca Language Rules

## Functions

Every function has three sections: logic, crash, test.

```roca
fn name(params) -> ReturnType {
    // logic — the happy path
    crash { /* error handlers */ }
    test { /* proof */ }
}
```

- `fn` = private, `pub fn` = exported
- Every function MUST have a `test {}` block
- Every function call MUST appear in the `crash {}` block
- Functions that can fail return `value, err`

## Test Blocks

```roca
test {
    self(1, 2) == 3          // assert equality
    self("a@b.com") is Ok    // assert no error
    self("") is err.missing   // assert specific error
}
```

- `self()` calls the function being tested
- Tests can ONLY call `self()` — nothing else
- Empty `test {}` is allowed for instance methods (tested via integration)

## Crash Blocks

```roca
crash {
    name.trim -> halt              // let error propagate
    db.save -> retry(3, 1000)      // retry 3 times, 1s apart
    parse -> skip                  // ignore failure
    fetch -> fallback("default")   // use default value
    http.get {                     // per-error handling
        err.timeout -> retry(3, 1000)
        err.not_found -> fallback("empty")
        default -> halt
    }
}
```

Strategies: `halt`, `skip`, `retry(n, ms)`, `fallback(value)`

## Variables

- `const x = 5` — immutable, cannot reassign
- `let x = 5` — mutable, can reassign
- `let result, err = fn()` — destructure error tuple

## Errors

Defined in contracts/structs with name + message:
```roca
err missing = "value is required"
```

Used in code as `return err.missing`, tested as `self("") is err.missing`

## Visibility

- `fn` / `struct` = private (not exported)
- `pub fn` / `pub struct` = exported
"#;

const SKILL_CONTRACTS: &str = r#"# Roca Contracts & Structs

## Contracts — What Must Be Done

A contract declares signatures, errors, and mocks. No implementation.

```roca
contract Stringable {
    to_string() -> String
}

contract HttpClient {
    get(url: String) -> Response, err {
        err timeout = "request timed out"
        err not_found = "404 not found"
    }
    mock {
        get -> Response { status: 200, body: "{}" }
    }
}

contract StatusCode {
    200
    400
    404
    500
}
```

## Structs — How It's Done

A struct has two blocks: first `{}` is the contract (fields + signatures), second `{}` is the implementation.

```roca
pub struct Email {
    value: String
    validate(raw: String) -> Email, err {
        err missing = "required"
        err invalid = "bad format"
    }
}{
    fn validate(raw: String) -> Email, err {
        if raw == "" { return err.missing }
        if !raw.includes("@") { return err.invalid }
        return Email { value: raw }
        crash { raw.includes -> halt }
        test {
            self("a@b.com") is Ok
            self("") is err.missing
            self("bad") is err.invalid
        }
    }
}
```

## Satisfies — Linking Struct to Contract

One block per contract. Never chained. Always separate.

```roca
Email satisfies Stringable {
    fn to_string() -> String {
        return self.value
        test { self() == "test" }
    }
}
```

The compiler checks:
- Every method in the contract is implemented
- Signatures match exactly
- One `satisfies` block per contract
"#;

const SKILL_PATTERNS: &str = r#"# Roca Patterns

## Error Handling Pattern

```roca
pub fn process(input: String) -> Result, err {
    let validated, val_err = validate(input)
    if val_err { return err.invalid_input }

    let saved, save_err = db.save(validated)
    if save_err { return err.save_failed }

    return saved

    crash {
        validate -> halt
        db.save -> retry(3, 1000)
    }

    test {
        self("valid") is Ok
        self("") is err.invalid_input
    }
}
```

## Match Pattern

```roca
return match status {
    200 => "ok"
    404 => "not found"
    _ => "unknown"
}
```

## Struct Composition

```roca
pub struct User {
    name: String
    email: Email      // Email is another struct
    validate(name: String, email_raw: String) -> User, err { ... }
}{ ... }
```

## Import Pattern

```roca
import { Email } from "./email.roca"
import { User } from "./user.roca"
```

Imports compile to `import { X } from "./file.js"`.

## Array Pattern

```roca
const items = [1, 2, 3]
let total = 0
for item in items {
    total = total + item
}
return items[0]       // index access
return items.length   // length
```

## Contract → Struct → Satisfies Flow

1. Define the contract (what)
2. Implement the struct (how)
3. Link with satisfies (prove it)

```roca
contract Describable { describe() -> String }

pub struct Product {
    name: String
    price: Number
}{}

Product satisfies Describable {
    fn describe() -> String {
        return self.name + " $" + self.price.toString()
        crash { self.price.toString -> halt }
        test { self() == "Widget $10" }
    }
}
```
"#;
