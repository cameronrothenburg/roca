# Stdlib

All stdlib methods are safe -- they do not return errors and do not need crash block entries.

## String

| Method | Signature |
|--------|-----------|
| `trim()` | `-> String` |
| `trimStart()` | `-> String` |
| `trimEnd()` | `-> String` |
| `toUpperCase()` | `-> String` |
| `toLowerCase()` | `-> String` |
| `replace(search, replacement)` | `-> String` |
| `slice(start, end)` | `-> String` |
| `repeat(count)` | `-> String` |
| `includes(search)` | `-> Bool` |
| `startsWith(prefix)` | `-> Bool` |
| `endsWith(suffix)` | `-> Bool` |
| `indexOf(search)` | `-> Number` |
| `split(separator)` | `-> Array<String>` |
| `charAt(index)` | `-> String` |
| `toString()` | `-> String` |
| `to_log()` | `-> String` |

## Number

| Method | Signature |
|--------|-----------|
| `toString()` | `-> String` |
| `toFixed(digits)` | `-> String` |
| `to_log()` | `-> String` |

## Bool

| Method | Signature |
|--------|-----------|
| `toString()` | `-> String` |
| `to_log()` | `-> String` |

## Array\<T\>

| Method | Signature |
|--------|-----------|
| `length` | `Number` (property) |
| `includes(item)` | `-> Bool` |
| `indexOf(item)` | `-> Number` |
| `join(separator)` | `-> String` |
| `reverse()` | `-> Array<T>` |
| `push(item)` | `-> Number` |
| `pop()` | `-> T` |
| `concat(other)` | `-> Array<T>` |
| `map(callback)` | `-> Array` |
| `filter(callback)` | `-> Array<T>` |
| `find(callback)` | `-> T` |
| `slice(start, end)` | `-> Array<T>` |
| `forEach(callback)` | `-> Ok` |
| `some(callback)` | `-> Bool` |
| `every(callback)` | `-> Bool` |
| `flat()` | `-> Array` |
| `reduce(callback, initial)` | `-> T` |

## Map\<V\>

| Method | Signature |
|--------|-----------|
| `get(key)` | `-> V` |
| `set(key, value)` | `-> Ok` |
| `has(key)` | `-> Bool` |
| `delete(key)` | `-> Bool` |
| `keys()` | `-> Array<String>` |
| `values()` | `-> Array<V>` |
| `size` | `Number` (property) |

## Bytes

| Method | Signature |
|--------|-----------|
| `byteLength` | `Number` (property) |
| `at(index)` | `-> Number` |
| `slice(start, end)` | `-> Bytes` |
| `toString()` | `-> String` |
| `to_log()` | `-> String` |
| `toHex()` | `-> String` |
| `toBase64()` | `-> String` |
| `toArray()` | `-> Array<Number>` |

## Buffer

| Method | Signature |
|--------|-----------|
| `write(bytes)` | `-> Ok` |
| `writeString(str)` | `-> Ok` |
| `writeByte(byte)` | `-> Ok` |
| `toBytes()` | `-> Bytes` |
| `toString()` | `-> String` |
| `byteLength` | `Number` (property) |
| `clear()` | `-> Ok` |

## Optional\<T\>

| Method | Signature |
|--------|-----------|
| `isPresent()` | `-> Bool` |
| `unwrap()` | `-> T` |
| `unwrapOr(default)` | `-> T` |

## Loggable

Contract requiring `to_log() -> String`. The types `String`, `Number`, `Bool`, and `Bytes` all satisfy `Loggable`.

The functions `log()`, `error()`, and `warn()` require their arguments to satisfy `Loggable`.
