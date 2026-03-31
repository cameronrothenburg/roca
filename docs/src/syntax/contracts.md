# Contracts

Contracts declare **what** -- signatures, errors, and mocks. They contain no implementation.

## Basic contract

```roca
contract HttpClient {
    get(url: String) -> String, err {
        err timeout = "request timed out"
        err not_found = "404 not found"
    }
}
```

Methods list their error names. During proof tests, the compiler auto-stubs extern contracts with default return values derived from their type signatures -- no user-written mocks needed.

## Generic contracts

```roca
contract Array<T> {
    push(item: T) -> Number
    pop() -> T
    map(callback: T) -> Array
    filter(callback: T) -> Array<T>
}
```

With constraints on the type parameter:

```roca
contract Logger<T: Loggable> {
    add(item: T) -> Number
}
```

## Relationship to structs

Structs implement contracts. The struct's first block **is** the contract (fields + signatures). The second block is the implementation. See [Structs](./structs.md).

Alternatively, use `satisfies` to link an existing struct to a separate contract:

```roca
Email satisfies Loggable {
    fn to_log() -> String {
        return self.value
        test { self() == "test@example.com" }
    }
}
```

## Extern contracts

Contracts prefixed with `extern` describe JS runtime types. See [Extern Contracts](../integration/extern-contracts.md).

## Compiler rules

| Rule | Trigger |
|------|---------|
| `duplicate-err` | Duplicate error name in a contract |
| `generic-mismatch` | Wrong type for generic parameter |
| `constraint-violation` | Type does not satisfy a generic constraint |
