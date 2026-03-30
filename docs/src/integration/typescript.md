# TypeScript

Roca generates `.d.ts` files automatically alongside the compiled JavaScript output.

## RocaResult<T>

Error-returning functions are typed with `RocaResult<T>`:

```ts
type RocaError = { name: string; message: string };
type RocaResult<T> = { value: T; err: null } | { value: null; err: RocaError };
```

A function declared as `pub fn find(id: String) -> User, err` generates:

```ts
export declare function find(id: string): RocaResult<User>;
```

Async functions return `Promise<RocaResult<T>>`.

## Non-error functions

Functions without `err` in the return type generate plain return types:

```ts
export declare function greet(name: string): string;
```

## Using from TypeScript

```ts
import { create_account } from "my-roca-lib";

const result = create_account("cam", "cam@test.com");
if (result.err) {
    // result.err is { name: string, message: string }
    console.error(result.err.name);
} else {
    // result.value is narrowed to the success type
    console.log(result.value.name);
}
```
