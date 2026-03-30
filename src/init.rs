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

pub struct App {
    name: String
    create(name: String) -> App, err {
        err missing = "name is required"
    }
}{
    fn create(name: String) -> App, err {
        if name == "" { return err.missing }
        return App { name: name }

        test {
            self("Roca") is Ok
            self("") is err.missing
        }
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

## IMPORTANT: Before writing any Roca code, run:

```bash
roca man
```

This outputs the complete language manual with every feature, syntax, and example.
Read it fully before writing code. It covers contracts, structs, crash blocks,
error handling, async, extern declarations, the adapter pattern, and all compiler rules.

## Quick Reference

```bash
roca build              # check → build JS → run proof tests
roca check              # lint + type check without emitting
roca test               # build + test, then clean output
roca run                # build + execute via bun
roca man                # full language manual
```

## Key Rules

1. Function bodies are pure happy path — no error variables
2. ALL errors handled in crash blocks — never `if err`
3. Every function MUST have a `test {{}}` block
4. Every call MUST have a crash handler
5. `crash {{ fn -> fallback(fn(e) -> e.message) }}` — closures access the error
6. `extern contract` declares JS shapes — pass as explicit function params

@.claude/skills/roca-lang/SKILL.md
"#, name = name));

    // Skills use proper Claude Code format: directory/SKILL.md with frontmatter
    let skills_dir = root.join(".claude").join("skills");
    fs::create_dir_all(skills_dir.join("roca-lang")).unwrap_or_else(|e| {
        eprintln!("error creating skills dir: {}", e);
        std::process::exit(1);
    });
    write(&skills_dir.join("roca-lang").join("SKILL.md"), SKILL_LANG);

    println!("✓ created {}", name);
    println!("  cd {} && roca build src/", name);
}

const SKILL_LANG: &str = r#"---
name: roca-lang
description: Roca language reference. Run `roca man` for the full manual. Use when writing, reviewing, or debugging Roca code.
---

# Roca Language

**Before writing any code, run `roca man` to read the full language manual.**

The manual covers every feature, syntax rule, and compiler error with examples.

## Quick Start

```bash
roca man                # read the full manual
roca build              # check → build JS → proof tests
roca check              # lint + type check only
```

## Core Principles

1. Function bodies are pure happy path — no error variables
2. ALL errors handled in crash blocks with strategies: halt, skip, fallback, retry, log, panic
3. `fallback(fn(e) -> expr)` — closure receives error with .name and .message
4. Every function has a test block — proof tests must pass before JS is emitted
5. Every call has a crash handler — the compiler enforces error handling
6. `extern contract` = JS dependency — pass as explicit function params, not env bags
7. `wait` is an expression: `const data = wait db.query(sql)`

## Crash Block Pattern

```roca
crash {
    db.query -> log |> retry(3, 1000) |> halt
}
```

## Extern Dependencies

```roca
extern contract Database {
    query(sql: String) -> String, err { err failed = "query failed" }
    mock { query -> "[]" }
}

pub fn handler(db: Database) -> String, err {
    err failed = "query failed"
    const data = wait db.query("SELECT * FROM users")
    return data
    crash {
        db.query -> halt
    }
    test { self(__mock_Database) is Ok }
}
```

Run `roca man` for the complete reference.
"#;

// Legacy constants kept for backward compat — not used in new projects
#[allow(dead_code)]
const SKILL_RULES: &str = r#"---
name: roca-rules
description: Roca language rules — functions, tests, crash blocks, variables, control flow, closures, async, extern, enums. Use when writing or reviewing Roca code.
---

# Roca Language Rules

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
- Async functions (with `wait`) are automatically awaited in tests

## Crash Blocks

Crash blocks declare how each call's errors are handled. Strategies chain with `|>`:

```roca
crash {
    name.trim -> skip                                // safe call, no wrapping
    db.save -> log |> retry(3, 1000) |> halt         // log, retry, then propagate
    analytics -> log |> skip                          // log and swallow
    config.load -> panic                              // crash the process
    fetch -> fallback("default")                      // use default value
    http.get {                                        // per-error handling
        err.timeout -> log |> retry(3, 1000) |> halt
        err.not_found -> fallback("empty")
        default -> log |> halt
    }
}
```

Steps: `log`, `retry(n, ms)`, `halt`, `skip`, `fallback(value)`, `fallback(fn(e) -> expr)`, `panic`

- `fallback(fn(e) -> expr)` — closure receives the error object with `.name` and `.message`
- Chains ending in `halt` → error propagates, caller must declare those errors
- Chains ending in `fallback`/`skip`/`panic` → error is consumed
- `halt` in `, err` functions auto-returns `(zero_value, err)` — no manual check needed

**Function bodies are pure happy path.** No error variables, no `if err` checks.
Errors are ONLY handled in crash blocks.

## Variables

- `const x = 5` — immutable, cannot reassign
- `let x = 5` — mutable, can reassign
- `self.field = value` — mutate struct fields in methods

## Errors

Errors are contracts with name + message:
```roca
err missing = "value is required"
```

Returns `{ name: "missing", message: "value is required" }`. Override the message at return:
```roca
return err.missing("name cannot be blank")
// { name: "missing", message: "name cannot be blank" }
```

Error returns include a zero value for the type: `("", err)` not `(null, err)`.
Tests match on `.name`: `self("") is err.missing`.
Errors are scoped per-method — different methods can reuse error names.

## Visibility

- `fn` / `struct` = private (not exported)
- `pub fn` / `pub struct` = exported

## Control Flow

- `if condition { } else { }` — conditional
- `for item in items { }` — iteration
- `while condition { }` — loops with `break` and `continue`
- `match value { pattern => result, _ => default }` — pattern matching
- Match arms can return errors: `match x { 200 => "ok", 404 => err.not_found, _ => err.unknown }`

## Closures

```roca
items.map(fn(x) -> x * 2)
items.filter(fn(x) -> x > 5)
const double = fn(x) -> x * 2
```

## Async (wait)

```roca
const data = wait http.get(url)
let a, b = waitAll { call1() call2() }
let fastest = waitFirst { call1() call2() }
```

Functions with `wait` auto-become async. No `async` keyword needed.
Errors from `wait` calls are handled by crash blocks.

## Null

- `null` is a keyword — use explicitly
- `Type | null` makes a field/param nullable
- Method calls on nullable values require a null check first

## Type Casts

```roca
let n, err = Number("42")   // safe — returns err on invalid
let s, err = String(42)     // safe — null returns err
```

## Enums

```roca
enum Status { active = "active", suspended = "suspended" }
```

Compiles to a const object: `const Status = { active: "active", suspended: "suspended" };`
"#;

#[allow(dead_code)]
const SKILL_CONTRACTS: &str = r#"---
name: roca-contracts
description: Roca contracts, structs, generics, extern declarations, satisfies blocks, enums. Use when defining types or implementing contracts.
---

# Roca Contracts & Structs

## Contracts — What Must Be Done

A contract declares signatures, errors, and mocks. No implementation.

```roca
contract Loggable {
    to_log() -> String
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
```

## Generic Contracts

Contracts can have type parameters with optional constraints:

```roca
contract Array<T> {
    push(item: T) -> Number
    pop() -> T
    map(callback: T) -> Array
    filter(callback: T) -> Array<T>
    includes(item: T) -> Bool
}

contract Logger<T: Loggable> {
    add(item: T) -> Number
}
```

The compiler enforces:
- `Array<Email>.push(42)` fails — 42 is not Email
- `Logger<Email>` fails if Email doesn't satisfy Loggable

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
        crash { raw.includes -> skip }
        test {
            self("a@b.com") is Ok
            self("") is err.missing
            self("bad") is err.invalid
        }
    }
}
```

Struct methods can mutate fields: `self.field = value`

## Satisfies — Linking Struct to Contract

One block per contract. Always separate.

```roca
Email satisfies Loggable {
    fn to_log() -> String {
        return self.value
        test { self() == "test" }
    }
}
```

The compiler checks:
- Every method in the contract is implemented
- Signatures match exactly
- One `satisfies` block per contract

## Field Constraints

Fields can declare inline constraints after the type:

```roca
pub struct User {
    name: String { min: 1, max: 64 }
    email: String { contains: "@", min: 3 }
    age: Number { min: 0, max: 150 }
    bio: String
}{}
```

Available: `min`, `max`, `minLen`, `maxLen`, `contains`, `pattern`.
Compiler rejects: `min > max`, `contains`/`pattern` on Number, any constraint on Bool.

## Extern Declarations

Declare JS runtime types and functions. The compiler type-checks them but emits no JS definitions.

```roca
extern contract HttpClient {
    get(url: String) -> Response, err {
        err timeout = "request timed out"
    }
}

extern contract Response {
    status: Number
    body: String
}

extern fn log(msg: String) -> Ok
```

- `extern contract` — describes a JS shape. Use as struct field types for the adapter pattern.
- `extern fn` — declares a JS function. Emits bare calls. Use for globals or imported functions.
- Mock blocks provide test doubles (externs don't exist at compile time).

**Adapter pattern** — bundle extern contracts into a struct, pass from JS:
```roca
pub struct Runtime { http: HttpClient }{}

pub fn handler(rt: Runtime) -> String {
    const resp = rt.http.get("/api")
    return resp.body
    crash {
        rt.http.get -> log |> fallback(fn(e) -> "error: " + e.message)
    }
}
```

Function body is pure happy path. Crash block handles all errors.
The JS side creates the adapter object and passes it in.

## Enums

```roca
enum Status { active = "active", suspended = "suspended" }
enum HttpCode { ok = 200, not_found = 404 }
```

Compiles to `const Status = { active: "active", suspended: "suspended" };`
"#;

#[allow(dead_code)]
const SKILL_PATTERNS: &str = r#"---
name: roca-patterns
description: Common Roca patterns — error handling, match, composition, imports, while loops, async wait, closures, nullable, extern, mutation. Use when writing Roca application logic.
---

# Roca Patterns

## Error Handling Pattern

Function bodies are pure happy path. Crash blocks handle all errors:

```roca
pub fn process(input: String) -> String, err {
    const validated = validate(input)
    const saved = db.save(validated)
    return saved

    crash {
        validate -> halt                          // propagate validate errors
        db.save -> log |> retry(3, 1000) |> halt  // log, retry, then propagate
    }

    test {
        self("valid") is Ok
        self("") is err.missing
    }
}
```

Use `fallback` with closures to access the error:
```roca
crash {
    db.query -> fallback(fn(e) -> Response.fail(500, e.message))
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

Match arms can return errors in functions that return `value, err`:

```roca
return match code {
    200 => "ok"
    404 => err.not_found
    _ => err.unknown
}
```

## Struct Mutation Pattern

```roca
pub struct Counter {
    count: Number
    increment() -> Counter
}{
    fn increment() -> Counter {
        self.count = self.count + 1
        return self
        test {}
    }
}
```

Use `self.field = value` to mutate fields within struct methods.

## Import Pattern

```roca
import { Email } from "./email.roca"
import { User } from "./user.roca"
```

Imports compile to `import { X } from "./file.js"`.

## Array Pattern

```roca
const items = [1, 2, 3]
const doubled = items.map(fn(x) -> x * 2)
const evens = items.filter(fn(x) -> x > 1)
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
        crash { self.price.toString -> skip }
        test { self() == "Widget $10" }
    }
}
```

## While Loop Pattern

```roca
let attempts = 0
while attempts < 3 {
    const result = try_connect()
    break
    attempts = attempts + 1
}
```

## Async Wait Pattern

```roca
const response = wait http.get("/api/data")

let users, posts = waitAll {
    db.getUsers()
    db.getPosts()
}
```

Async errors are handled by crash blocks, not in the function body.

## Closure Pattern

```roca
const doubled = items.map(fn(x) -> x * 2)
const valid = items.filter(fn(x) -> x != null)
```

## Nullable Pattern

```roca
pub struct Profile {
    name: String
    bio: String | null
}

if profile.bio != null {
    log(profile.bio)
}
```

## Constrained Types Pattern

```roca
pub struct Registration {
    username: String { min: 3, max: 32, pattern: "[a-zA-Z0-9_]" }
    email: String { contains: "@", min: 3 }
    age: Number { min: 13, max: 150 }
    bio: String
}{}
```

## Extern Dependencies

Declare JS shapes with `extern contract`. Pass as explicit function parameters — not env bags:

```roca
extern contract Database {
    query(sql: String) -> String, err {
        err failed = "query failed"
    }
    mock { query -> "[]" }
}

extern contract Logger {
    info(msg: String) -> Ok
    mock { info -> Ok }
}

pub fn get_users(db: Database, logger: Logger) -> String, err {
    err failed = "query failed"
    const data = wait db.query("SELECT * FROM users")
    logger.info("fetched users")
    return data
    crash {
        db.query -> halt
    }
    test { self(__mock_Database, __mock_Logger) is Ok }
}
```

```js
// JS side — pass dependencies directly
import { get_users } from "./out/app.js";

const db = {
    query: async (sql) => {
        const rows = await pool.query(sql);
        return { value: JSON.stringify(rows), err: null };
    }
};
const logger = { info: (msg) => console.log(msg) };

const { value, err } = await get_users(db, logger);
```

Extern contracts can also be used standalone with `extern fn` for globals:
```roca
extern fn log(msg: String) -> Ok
```
"#;
