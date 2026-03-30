# JS Wiring

Roca compiles to JavaScript. This page covers the protocol for calling Roca functions from JS and implementing extern contracts.

## The {value, err} protocol

Roca functions that return errors use `{value, err}` objects -- **not** `[value, err]` tuples.

```js
import { create_account } from "my-roca-lib";

const { value: user, err } = create_account("cam", "cam@test.com");
if (err) {
    console.error(err.name, err.message);
} else {
    console.log(user.name);
}
```

The error object has two fields: `name` (string identifier) and `message` (human-readable text).

## Implementing extern contracts

Extern contract methods that declare errors must return `{value, err}`:

```js
const db = {
    query: async (sql) => {
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

Extern methods **without** errors return plain values:

```js
const logger = {
    info: (msg) => console.log(msg)
};
```

## Async functions

Functions that use `wait` compile to async functions. Call them with `await`:

```js
const { value, err } = await get_users(db);
```

## Cloudflare Workers example

```js
import { get_users } from "my-roca-lib";

export default {
    async fetch(request, env) {
        const db = {
            query: async (sql) => {
                const rows = await env.DB.prepare(sql).all();
                return { value: JSON.stringify(rows.results), err: null };
            }
        };

        const { value, err } = await get_users(db);
        if (err) return new Response(err.message, { status: 500 });
        return new Response(value);
    }
};
```
