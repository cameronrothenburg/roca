# Using Roca in Your Project

Roca compiles `.roca` files to JavaScript. There are two ways to use the output.

## Option 1: Direct import from `out/`

The simplest approach. `roca build` writes JS to the `out/` directory:

```
my-roca-lib/
  src/
    validate.roca
  out/
    validate.js       ← compiled JS
    validate.d.ts     ← TypeScript declarations
  roca.toml
```

Import directly from the output:

```ts
import { validate } from "./my-roca-lib/out/validate.js";

const { value, err } = validate("cam@test.com");
```

Good for: monorepos, quick prototyping, single-file libraries.

## Option 2: jslib mode (npm package)

Set `mode = "jslib"` in `roca.toml`:

```toml
[project]
name = "my-lib"
version = "0.1.0"

[build]
src = "src/"
out = "out/"
mode = "jslib"
```

Now `roca build` generates `out/package.json` and runs `npm install`, making the lib available as a node module:

```
my-lib/
  src/
    validate.roca
    types.roca
  out/
    validate.js
    validate.d.ts
    types.js
    types.d.ts
    package.json      ← generated, name from roca.toml
  roca.toml
```

The generated `package.json`:

```json
{
  "name": "my-lib",
  "version": "0.1.0",
  "type": "module",
  "main": "main.js",
  "types": "main.d.ts",
  "exports": {
    ".": "./main.js",
    "./validate": "./validate.js",
    "./types": "./types.js"
  },
  "files": ["*.js", "*.d.ts"]
}
```

Import as a package:

```ts
import { validate } from "my-lib";
import { Email } from "my-lib/types";
```

Good for: shared libraries, publishing, workspace packages, Cloudflare Workers.

## What gets generated

For each `.roca` file, the compiler outputs:

| File | Contents |
|------|----------|
| `name.js` | Compiled JavaScript (ESM) |
| `name.d.ts` | TypeScript declarations with `RocaResult<T>` types |

The `.d.ts` includes:
- Exported functions with parameter and return types
- Struct classes with fields, constructors, and methods
- Extern contract interfaces (so TS validates your adapters)
- `RocaResult<T>` and `RocaError` shared types

## Step-by-step: adding Roca to a TypeScript project

### 1. Create the Roca lib

```bash
mkdir libs/my-roca-lib && cd libs/my-roca-lib
roca init .
```

Edit `roca.toml`:
```toml
[project]
name = "my-roca-lib"

[build]
mode = "jslib"
```

### 2. Write Roca code

`src/account.roca`:
```roca
/** Validates and creates a user account */
pub fn create_account(name: String, email: String) -> Account, err {
    err missing_name = "name is required"
    err missing_email = "email is required"
    err invalid_email = "email must contain @"

    if name == "" { return err.missing_name }
    if email == "" { return err.missing_email }
    if !email.includes("@") { return err.invalid_email }
    return Account { name: name, email: email }

    test {
        self("cam", "cam@test.com") is Ok
        self("", "cam@test.com") is err.missing_name
        self("cam", "") is err.missing_email
        self("cam", "bad") is err.invalid_email
    }
}

/** A validated user account */
pub struct Account {
    name: String
    email: String
}{}
```

### 3. Build

```bash
roca build
```

Output:
```
checking 1 file(s)...
building 1 file(s)...
testing...
110 passed, 0 failed across 1 file(s)
✓ all files built → ./out/
installing my-roca-lib into node_modules...
✓ installed — import { } from "my-roca-lib"
```

### 4. Use from TypeScript

```ts
import { create_account } from "my-roca-lib";

const { value: account, err } = create_account("cam", "cam@test.com");
if (err) {
    // err.name is "missing_name" | "missing_email" | "invalid_email"
    // err.message is the human-readable text
    console.error(err.name, err.message);
} else {
    console.log(account.name, account.email);
}
```

TypeScript knows the types — `account` is `Account`, `err` is `RocaError | null`. No `as any` casts needed.

### 5. Rebuild on changes

After editing `.roca` files:

```bash
roca build
```

The output is updated in place. No restart needed for dev servers that watch `node_modules`.

## Passing extern dependencies

If your Roca code uses extern contracts, wire them from JS:

```roca
/** Database connection */
pub extern contract Database {
    /// Run a query
    query(sql: String) -> String, err {
        err query_failed = "query failed"
    }
}

/** Fetch all users */
pub fn get_users(db: Database) -> String, err {
    err query_failed = "query failed"
    const data = wait db.query("SELECT * FROM users")
    return data
    crash { db.query -> halt }
    test { self(Database) is Ok }
}
```

Wire the adapter in JS/TS:

```ts
import { get_users } from "my-roca-lib";

const db = {
    query: async (sql: string) => {
        try {
            const rows = await pool.query(sql);
            return { value: JSON.stringify(rows), err: null };
        } catch (e) {
            return { value: null, err: { name: "query_failed", message: e.message } };
        }
    }
};

const { value, err } = await get_users(db);
```

The generated `.d.ts` exports the `Database` interface, so TypeScript validates your adapter at compile time.
