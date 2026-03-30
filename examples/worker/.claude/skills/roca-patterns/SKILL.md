---
name: roca-patterns
description: Common Roca patterns — error handling, match, composition, imports, while loops, async wait, closures, nullable, extern, mutation. Use when writing Roca application logic.
---

# Roca Patterns

## Error Handling Pattern

Crash blocks handle errors — no manual `if err` needed for propagation:

```roca
pub fn process(input: String) -> String, err {
    let validated, v_err = validate(input)
    let saved, s_err = db.save(validated)
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

`halt` on a `let val, err = call()` auto-returns `(zero_value, err)` — no manual check needed.

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
        crash { self.price.toString -> halt }
        test { self() == "Widget $10" }
    }
}
```

## While Loop Pattern

```roca
let attempts = 0
while attempts < 3 {
    let result, err = try_connect()
    if err == null { break }
    attempts = attempts + 1
}
```

## Async Wait Pattern

```roca
let response, failed = wait http.get("/api/data")
if failed { return err.fetch_failed }

let users, posts, failed = waitAll {
    db.getUsers()
    db.getPosts()
}
```

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

## Extern + Adapter Pattern

Declare JS shapes with `extern contract`, bundle them into an adapter struct, pass from JS:

```roca
extern contract HttpClient {
    get(url: String) -> String, err {
        err network = "network error"
    }
}

extern contract Database {
    query(sql: String) -> String, err {
        err failed = "query failed"
    }
}

// Adapter bundles externs — dependency injection
pub struct Services {
    http: HttpClient
    db: Database
}{}

pub fn handle(svc: Services, path: String) -> String, err {
    let data, err = svc.db.query("SELECT * FROM " + path)
    return data
    crash {
        svc.db.query -> log |> retry(3, 1000) |> halt
    }
    test { self(Services { http: null, db: null }, "/users") == "ok" }
}
```

```js
// JS side — wire once, pass everywhere
import { handle } from "./out/app.js";

const svc = {
    http: { get: (url) => fetch(url).then(r => [r.text(), null]) },
    db: { query: (sql) => pool.query(sql) }
};

handle(svc, "/users");
```

Extern contracts can also be used standalone with `extern fn` for globals:
```roca
extern fn log(msg: String) -> Ok
```
