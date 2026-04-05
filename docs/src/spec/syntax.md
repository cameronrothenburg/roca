# Roca Syntax

This is the complete syntax reference for the Roca language as currently implemented.

---

## Source File

A source file is a sequence of items:

```
SourceFile = Item*
Item = Import | Function | Struct | Enum
```

## Import

```roca
import { User, Config } from "./types.roca"
```

Paths must be relative with `.roca` extension. Compiled JS output changes `.roca` to `.js`.

## Function

```roca
pub fn add(b a: Int, b b: Int) -> Int {
    return a + b
test {
    self(1, 2) == 3
    self(0, 0) == 0
}}
```

- `pub` — exported (optional)
- Parameters require `o` (owned) or `b` (borrowed) qualifier
- `-> Type` — return type (required)
- `test { }` — inline proof tests (optional)

## Parameters

Every parameter declares ownership intent:

```roca
fn process(b config: Config, o data: Data) -> Result
```

- `b` — **borrowed**: caller keeps ownership, function reads only
- `o` — **owned**: caller transfers ownership, function takes it

Omitting `o`/`b` is a compile error (E-OWN-005).

## Types

```
Type = Int | Float | String | Bool | Unit
     | Named       (e.g. User, Point)
     | Array(Type)
     | Fn(Types, Type)
     | Optional(Type)
```

## Struct

Two-block syntax: fields, then methods.

```roca
pub struct Point { x: Int  y: Int }{
    pub fn new(o x: Int, o y: Int) -> Point {
        return Point { x: x, y: y }
    }
    pub fn get_x() -> Int {
        return self.x
    }
}
```

- Fields in the first block
- Methods in the second block
- `self` refers to the instance in methods
- Struct literals (`Point { x: 1, y: 2 }`) only inside struct methods

## Enum

```roca
pub enum Color {
    Red
    Green
    Blue
}

pub enum Result {
    Ok(String)
    Err(String)
}
```

Unit variants carry no data. Data variants carry typed values.

## Statements

```
Stmt = const name = expr          // immutable, owns the value
     | var name = expr            // mutable, owns the value
     | let name = expr            // borrow from a const
     | name = expr                // reassign a var
     | target.field = expr        // set struct field
     | return expr                // return from function
     | if expr { stmts } else { stmts }
     | loop { stmts }            // infinite loop
     | for name in expr { stmts }
     | break
     | continue
     | expr                       // expression statement
```

### Bindings

- `const` creates an **owned** value — the caller is responsible for its lifetime
- `let` creates a **borrow** — must derive from an existing `const`, not create new values
- `var` creates a **mutable owned** value — can be reassigned

```roca
const user = User.new("alice", 30)   // owned
let name = user.name                  // borrow
var count = 0                         // mutable owned
count = count + 1                     // reassign
```

### Error Handling

No crash blocks. Use `let val, err = call()` for error-returning functions:

```roca
fn load(b path: String) -> String {
    let result, err = read_file(path)
    if err {
        return ""
    }
    return result
}
```

## Expressions

```
Expr = literal                    // 42, 3.14, "hello", true, false
     | ident                      // variable reference
     | expr op expr               // binary: +, -, *, /, %, ==, !=, <, >, <=, >=, &&, ||
     | op expr                    // unary: !, -
     | expr(args)                 // function call
     | expr.field                 // field access
     | expr[index]                // array index
     | Name { field: expr, ... }  // struct literal (inside methods only)
     | Name.Variant(args)         // enum variant
     | [expr, ...]                // array literal
     | fn(params) -> expr         // closure
     | match expr { arms }        // pattern match
     | if expr { expr } else { expr }  // conditional expression
     | wait expr                  // async
     | self                       // struct instance reference
```

### Operator Precedence (high to low)

| Precedence | Operators |
|------------|-----------|
| 1 (highest) | `!`, `-` (unary) |
| 2 | `*`, `/`, `%` |
| 3 | `+`, `-` |
| 4 | `<`, `>`, `<=`, `>=` |
| 5 | `==`, `!=` |
| 6 | `&&` |
| 7 (lowest) | `\|\|` |

### Match

```roca
const result = match n {
    1 => "one"
    2 => "two"
    Result.Ok(val) => val
    _ => "other"
}
```

Arms: literal patterns, enum variant patterns with bindings, or `_` wildcard.

### Closures

```roca
const double = fn(x) -> x * 2
items.map(fn(item) -> item + 1)
```

Arrow function with untyped parameters.

## Test Blocks

Every `pub fn` should have a test block:

```roca
pub fn add(b a: Int, b b: Int) -> Int {
    return a + b
test {
    self(1, 2) == 3
    self(0, 0) == 0
    self(-1, 1) == 0
}}
```

`self(args) == expected` calls the function and asserts equality.
