# Extern Contracts

Extern contracts describe JavaScript runtime types. No JS is emitted for them -- they exist only to give the Roca compiler type information about external dependencies.

## Declaration

```roca
extern contract Database {
    query(sql: String) -> String, err {
        err query_failed = "database query failed"
    }
}
```

- Methods can declare errors with `-> Type, err`.
- The compiler auto-stubs extern contracts during proof tests using default return values derived from their type signatures.

## Extern functions

Standalone extern functions:

```roca
extern fn log(msg: String) -> Ok
```

## Usage as parameters

External dependencies are passed as **explicit function parameters** -- not bundled in an environment bag:

```roca
/// Fetches all users from the database
pub fn get_users(db: Database) -> String, err {
    err query_failed = "database query failed"
    const data = wait db.query("SELECT * FROM users")
    return data
    crash {
        db.query -> halt
    }
    test {
        self(Database) is Ok
    }
}
```

The function signature declares exactly what it needs. The caller provides the real implementation at runtime.

## Auto-stubs in tests

In test blocks, pass the contract name directly. The compiler generates a stub with default return values derived from the contract's type signatures:

```roca
test {
    self(Database) is Ok
}
```

For methods returning `String`, the stub returns `""`. For `Number`, `0`. For structs, a default instance. For error-returning methods, the stub returns the success case.

## Generating from TypeScript

If you have TypeScript declaration files (`.d.ts`), generate extern contracts automatically:

```bash
roca gen-extern worker-configuration.d.ts
```

This parses the TypeScript interfaces and generates a `.roca` file with:
- Type mapping (`string` → `String`, `Promise<T>` → async, `T | null` → `Optional<T>`)
- Error inference from method names (`get` → `not_found`, `put` → `failed`)
- Cross-references between interfaces resolved

Works with Cloudflare `wrangler types` output and any `.d.ts` file.

## Wiring from JS

See [JS Wiring](./js-wiring.md) for how to implement extern contracts on the JavaScript side.
