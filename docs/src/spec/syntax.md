# 2. Syntax

This section defines the syntactic structure of Roca programs. A conforming parser MUST accept programs that follow the grammar defined here and MUST reject programs that do not.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

---

## 2.1 Source File

A source file is a sequence of zero or more top-level items. A conforming parser MUST consume all tokens until `EOF` and produce a `SourceFile` node containing the ordered list of items.

```text
SourceFile = Item*
```

```roca
import { HttpClient } from std::http

contract Fetchable {
    fetch(url: String) -> String, err
}

pub fn main() -> Number {
    return 0
}
```

Items MAY appear in any order. A source file MAY be empty.

---

## 2.2 Items

An item is a top-level declaration. The parser MUST recognize the following item forms:

```text
Item = Import
     | Contract
     | ExternContract
     | Enum
     | Struct
     | Satisfies
     | Function
     | ExternFn
```

### 2.2.1 Import

An import brings names from another `.roca` file into scope.

```text
Import = 'import' '{' Ident (',' Ident)* '}' 'from' StringLit
```

The import list MUST contain at least one name. The source MUST be a string literal with a relative path.

```roca
// Import from a relative .roca file in the same project
import { UserProfile, Permissions } from "./types.roca"

// Import from a subdirectory
import { DatabaseClient } from "./db/client.roca"
```

Paths MUST be relative (starting with `./` or `../`) and MUST use the `.roca` extension. The compiled JS output replaces `.roca` with `.js`:

```javascript
// Compiled output
import { UserProfile, Permissions } from "./types.js";
import { DatabaseClient } from "./db/client.js";
```

Core type contracts (String, Number, Bool, Array, Map, Optional, Bytes) are always available -- no import needed. Module-specific stdlib contracts (Math, Fs, Http, JSON, etc.) MUST be imported with `import { Name } from std::module`. See [Section 4](./modules.md) for details.

### 2.2.2 Contract

A contract declares a set of capabilities that a type must implement. It defines function signatures, fields, and optional type parameters.

```text
Contract = 'pub'? 'contract' Ident TypeParams? '{' (FnSignature | Field)* '}'
TypeParams = '<' TypeParam (',' TypeParam)* '>'
TypeParam = Ident (':' Ident)?
```

The `pub` modifier is OPTIONAL. Type parameters MAY have a constraint (another contract name) separated by `:`.

```roca
contract Stringable {
    toString() -> String
}

pub contract Collection<T> {
    length() -> Number
    get(index: Number) -> T | null
    push(item: T) -> self
}

contract Comparable<T: Orderable> {
    compare(other: T) -> Number
}
```

### 2.2.3 Extern Contract

An extern contract declares a type provided by the runtime environment. It MUST NOT be emitted as generated code.

```text
ExternContract = 'pub'? 'extern' 'contract' Ident TypeParams? '{' (FnSignature | Field)* '}'
```

Extern contracts SHOULD use the `pub` modifier. The parser accepts non-pub extern contracts.

```roca
pub extern contract Console {
    log(message: String) -> null
    error(message: String) -> null
}
```

### 2.2.4 Enum

An enum defines a type with a fixed set of named variants. Enums come in two forms: flat (key-value) and algebraic (data-carrying).

```text
Enum = 'pub'? 'enum' Ident '{' EnumVariants '}'
EnumVariants = FlatVariants | AlgebraicVariants
FlatVariants = Ident '=' (StringLit | NumberLit) (',' Ident '=' (StringLit | NumberLit))*
AlgebraicVariants = AlgebraicVariant (',' AlgebraicVariant)* | AlgebraicVariant+
AlgebraicVariant = Ident ('(' TypeRef (',' TypeRef)* ')')?
```

Flat variants MUST have a string or number value. Algebraic variants MAY carry typed data.

```roca
// Flat enum with string values
pub enum Color {
    Red = "red",
    Green = "green",
    Blue = "blue"
}

// Flat enum with number values
enum Priority {
    Low = 0,
    Medium = 1,
    High = 2
}

// Algebraic enum with data variants
pub enum Result {
    Ok(String)
    Err(String)
    Loading
}
```

### 2.2.5 Struct

A struct defines a named data type with fields, function signatures, and method implementations. Structs use a two-block syntax: the first block declares the contract (fields and signatures), the second block provides implementations.

```text
Struct = 'pub'? 'struct' Ident '{' (Field | FnSignature)* '}' '{' FnDef* '}'
```

The first block MUST contain fields and/or function signatures. The second block MUST contain function definitions that implement the declared signatures.

```roca
pub struct Email {
    value: String { contains: "@", maxLen: 255 }

    toString() -> String
}{
    fn toString() -> String {
        return self.value
    }
}

pub struct Counter {
    count: Number { min: 0, default: "0" }

    increment() -> self
    getCount() -> Number
}{
    fn increment() -> self {
        self.count = self.count + 1
        return self
    }

    fn getCount() -> Number {
        return self.count
    }
}
```

### 2.2.6 Satisfies

A satisfies block provides implementations of a contract for a specific struct.

```text
Satisfies = Ident 'satisfies' Ident TypeArgs? '{' FnDef* '}'
TypeArgs = '<' TypeRef (',' TypeRef)* '>'
```

The first identifier MUST name an existing struct. The second MUST name an existing contract.

```roca
Email satisfies Stringable {
    fn toString() -> String {
        return self.value
    }
}

NumberList satisfies Collection<Number> {
    fn length() -> Number {
        return self.items.length
    }

    fn get(index: Number) -> Number | null {
        return self.items[index]
    }

    fn push(item: Number) -> self {
        self.items = self.items.concat([item])
        return self
    }
}
```

### 2.2.7 Function

A function is the primary unit of computation. See section 2.3 for the full definition.

### 2.2.8 Extern Function

An extern function declares a function provided by the runtime. The body contains only error declarations.

```text
ExternFn = 'pub'? 'extern' 'fn' Ident '(' Params ')' '->' TypeRef (',' 'err')? '{' ErrDecl* '}'
```

Extern functions SHOULD use the `pub` modifier. They MUST NOT have a body, crash block, or test block — only error declarations.

```roca
pub extern fn fetch(url: String) -> String, err {
    err network_error = "failed to reach server"
    err timeout = "request timed out"
}

pub extern fn readFile(path: String) -> String, err {
    err not_found = "file does not exist"
    err permission_denied = "cannot read file"
}
```

---

## 2.3 Function Definition

A function definition is the primary executable item. It consists of a signature, a body, and an optional test block.

```text
FnDef = DocComment? 'pub'? 'fn' Ident TypeParams? '(' Params ')' '->' TypeRef (',' 'err')? '{'
            ErrDecl*
            Stmt*
        'test' '{'
            TestCase*
        '}'
        '}'
```

### 2.3.1 Visibility

The `pub` modifier is OPTIONAL. If present, the function is exported from its module. If absent, the function is module-private.

```roca
// Exported
pub fn add(a: Number, b: Number) -> Number {
    return a + b
}

// Module-private
fn helper(x: Number) -> Number {
    return x * 2
}
```

### 2.3.2 Parameters

Parameters are a comma-separated list of ownership-qualified name-type pairs. Every parameter MUST declare its ownership intent with `o` (owned/consumed) or `b` (borrowed/read-only). Parameters MAY include constraints in curly braces after the type.

```text
Params = (Param (',' Param)*)?
Param = OwnershipQualifier Ident ':' TypeRef ('{' Constraint (',' Constraint)* '}')?
OwnershipQualifier = 'o' | 'b'
Constraint = Ident ':' (NumberLit | StringLit)
```

- `b` — **borrowed**. The caller retains ownership. The function reads but does not consume.
- `o` — **owned**. The caller transfers ownership. The function is responsible for the value.

```roca
// Borrow both — caller keeps them
pub fn greet(b name: String, b age: Number) -> String {
    return "Hello " + name
}

// Take ownership — caller loses file after this call
pub fn consume(o file: File) -> Ok {
    return Ok
}

// Mixed — borrow config, consume data
pub fn process(b config: Config, o data: Data) -> Result {
    return transform(data)
}

// With constraints
pub fn clamp(
    b value: Number,
    b low: Number { max: 1000 },
    b high: Number { min: 0 }
) -> Number {
    return value
}
```

See [Section 5 — Memory Model](./memory.md) for the full ownership rules.

### 2.3.3 Return Type

Every function SHOULD declare a return type with `->`. If omitted, the return type defaults to `Ok`. If the function can return errors, the return type MUST be followed by `, err`.

```roca
// Returns a value only
pub fn add(a: Number, b: Number) -> Number {
    return a + b
}

// Returns a value or an error
pub fn divide(a: Number, b: Number) -> Number, err {
    err division_by_zero = "cannot divide by zero"
    if b == 0 {
        return err.division_by_zero
    }
    return a / b
}
```

### 2.3.4 Error Declarations

Error declarations name the errors a function can return, with a human-readable message.

```text
ErrDecl = 'err' Ident '=' StringLit
```

Error declarations MAY appear in:
- Function bodies (at the top, before statements)
- Contract function signatures
- Extern fn blocks

In the function body, errors are returned via `return err.name` or `return err.name("custom message")`. The declared names define the complete set of errors the function can produce.

```roca
// Error declarations in an extern fn
pub extern fn fetch(url: String) -> String, err {
    err network_error = "failed to reach server"
    err timeout = "request timed out"
}

// In function bodies, errors are returned — not declared
pub fn divide(a: Number, b: Number) -> Number, err {
    err division_by_zero = "cannot divide by zero"
    if b == 0 {
        return err.division_by_zero
    }
    return a / b
}
```

### 2.3.5 Body

The function body contains the happy-path logic. It MUST contain only statements and expressions that handle the success case. Error conditions are signaled by returning `err.name`.

### 2.3.6 Error Handling

There is no crash block. Error handling uses `let val, err = call()` inline. Every call to an error-returning function MUST have its error handled — either by checking it, returning it, or using a stdlib helper.

```roca
pub fn load_user(b id: String) -> User, err {
    err not_found = "user not found"

    let response, failed = Http.get("/users/" + id)
    if failed { return err.not_found }

    const user = User { name: response }
    return user
}
```

Built-in helpers for common patterns:

```roca
// Retry with attempts and delay
let data, failed = retry(3, 1000, fn() -> Http.get(url))
if failed { return err.network }

// Fallback to a default value
const config = fallback(load_config(path), Config.default())

// Log and continue
let result, failed = db.query(sql)
if failed { log(failed) }
```

A conforming compiler MUST reject any function that calls an error-returning function without handling the error. See [Section 7 — Errors](./errors.md) for the full error code reference.

### 2.3.7 Test Block

The test block contains proof assertions for the function. It appears after the body, inside the function's closing brace.

```text
TestBlock = 'test' '{' TestCase* '}'
TestCase = 'self' '(' Args ')' ('==' Expr | 'is' 'err' '.' Ident | 'is' 'Ok')
```

Test cases call the function using `self` and assert value equality (`==`), error identity (`is err.name`), or success (`is Ok`).

```roca
pub fn add(a: Number, b: Number) -> Number {
    return a + b
test {
    self(1, 2) == 3
    self(0, 0) == 0
    self(-1, 1) == 0
}}

pub fn divide(a: Number, b: Number) -> Number, err {
    err division_by_zero = "cannot divide by zero"
    if b == 0 {
        return err.division_by_zero
    }
    return a / b
test {
    self(10, 2) == 5
    self(0, 1) == 0
    self(1, 0) is err.division_by_zero
    self(10, 5) is Ok
}}
```

### 2.3.8 Block Ordering

Within a function's braces, elements MUST appear in this order:

1. Error declarations (`err name = "message"`)
2. Body statements
3. Test block (OPTIONAL)

---

## 2.4 Statements

Statements are the executable units within function bodies, control flow blocks, and struct methods.

```text
Stmt = ConstDecl | LetDecl | Assignment | Return | If | While | For | Break | Continue
     | FieldAssignment | Wait | WaitAll | WaitFirst | ExprStmt
```

### 2.4.1 Const Declaration

An immutable binding. The value MUST NOT be reassigned after declaration.

```text
ConstDecl = 'const' Ident (':' TypeRef)? '=' Expr
```

The type annotation is OPTIONAL. If provided, it follows the binding name after a colon.

```roca
const name = "roca"
const count: Number = items.length
const result = add(1, 2)
```

### 2.4.2 Let Declaration

A mutable binding. The value MAY be reassigned.

```text
LetDecl = 'let' Ident (':' TypeRef)? '=' Expr
        | 'let' Ident ',' Ident '=' Expr
```

The type annotation is OPTIONAL. If provided, it follows the binding name after a colon.

The destructuring form `let name, err_name = expr` binds both the success value and the error from an error-returning call. This is the idiomatic way to handle calls that return `, err`.

```roca
let count = 0
let message: String = "initial"
let result, parseErr = parse(input)
```

### 2.4.3 Assignment

Reassignment of a `let` binding. The target MUST have been declared with `let`. Assigning to a `const` binding is a compile error.

```text
Assignment = Ident '=' Expr
```

```roca
let count = 0
count = count + 1
count = count * 2
```

### 2.4.4 Return

Returns a value from the enclosing function. A function body MUST contain at least one `return` statement on every code path.

```text
Return = 'return' Expr
       | 'return' 'err' '.' Ident ('(' StringLit ')')?
```

The expression MAY be a value or an error reference. Error returns MAY include an optional custom message argument.

```roca
return 42
return "hello"
return err.not_found
return err.not_found("user 123 was not found")
return Ok(result)
```

### 2.4.5 If / Else

Conditional execution. The condition MUST be an expression. The `else` branch is OPTIONAL. The parser requires `{` after `else` — `else if` chaining is NOT supported. To express multi-branch conditions, use `match` or nest `if` inside `else`.

```text
If = 'if' Expr '{' Stmt* '}' ('else' '{' Stmt* '}')?
```

```roca
if age >= 18 {
    return "adult"
} else {
    return "minor"
}

if status == "active" {
    const user = getUser(id)
    return user
}
```

### 2.4.6 While Loop

Repeated execution while a condition holds.

```text
While = 'while' Expr '{' Stmt* '}'
```

```roca
let attempts = 0
while attempts < 3 {
    const result = tryConnect()
    if result != null {
        return result
    }
    attempts = attempts + 1
}
```

### 2.4.7 For Loop

Iteration over a collection.

```text
For = 'for' Ident 'in' Expr '{' Stmt* '}'
```

The binding name is scoped to the for block and is immutable within each iteration.

```roca
for item in items {
    const processed = transform(item)
    results = results.concat([processed])
}

for i in range(0, 10) {
    total = total + i
}
```

### 2.4.8 Break and Continue

`break` exits the nearest enclosing loop. `continue` skips to the next iteration.

```text
Break = 'break'
Continue = 'continue'
```

These statements SHOULD only appear inside a `while` or `for` loop body. The parser does not enforce this constraint.

```roca
for item in items {
    if item == null {
        continue
    }
    if item.priority == "critical" {
        break
    }
}
```

### 2.4.9 Field Assignment

Assigns a value to a field on a struct instance.

```text
FieldAssignment = ('self' | Ident) '.' Ident '=' Expr
```

Only `self.field = expr` and `ident.field = expr` are supported. Nested field assignment (e.g., `a.b.c = expr`) is NOT supported.

```roca
self.count = self.count + 1
user.name = newName
```

### 2.4.10 Wait, WaitAll, WaitFirst

Async operations for concurrent execution.

```text
Wait = 'wait' Expr
WaitAllDestructure = 'let' Ident (',' Ident)* ',' Ident '=' 'waitAll' '{' Expr+ '}'
WaitFirstDestructure = 'let' Ident (',' Ident)* ',' Ident '=' 'waitFirst' '{' Expr+ '}'
```

`wait` is an expression-level construct that suspends until a single async operation completes.

`waitAll` and `waitFirst` are NOT standalone block statements. They MUST appear in a destructuring `let` binding. Each result is bound to a name, and the final name in the binding captures failures.

```roca
// Single await (expression-level)
const response = wait http.get("/api/users")

// Concurrent — wait for all (destructuring let only)
let users, posts, failed = waitAll {
    http.get("/api/users")
    http.get("/api/posts")
}

// Concurrent — first to resolve wins (destructuring let only)
let primary, fallback, failed = waitFirst {
    db.query("SELECT * FROM cache")
    http.get("/api/data")
}
```

---

## 2.5 Expressions

Expressions produce values. They may appear on the right side of bindings, as function arguments, in conditions, and as return values.

```text
Expr = Literal | Ident | Binary | Unary | Call | FieldAccess | Index
     | Match | StructLit | Closure | Interpolation | EnumVariant | Wait | Array
```

### 2.5.1 Literals

Literal expressions produce constant values.

```roca
42              // Number
3.14            // Number (float)
"hello"         // String
'hello'         // String
true            // Boolean
false           // Boolean
null            // Null
[1, 2, 3]      // Array
```

### 2.5.2 Identifiers

An identifier expression resolves to the value bound to that name in the current scope.

```roca
const x = count
const y = userName
```

### 2.5.3 Binary Expressions

A binary expression applies an operator to two operands. See section 2.7 for precedence rules.

```text
Binary = Expr Op Expr
Op = '+' | '-' | '*' | '/' | '==' | '!=' | '<' | '>' | '<=' | '>=' | '&&' | '||'
```

```roca
const sum = a + b
const isValid = age >= 18 && hasPermission == true
const combined = firstName + " " + lastName
```

### 2.5.4 Unary Expressions

A unary expression applies a prefix operator to a single operand. Unary operators are `!` (logical not) and `-` (negation, desugared as `0 - expr`).

```text
Unary = ('!' | '-') Expr
```

```roca
if !isValid {
    return err.invalid
}
const negative = -count
const inverted = -1 * factor
```

### 2.5.5 Call Expressions

A call expression invokes a function or method with arguments.

```text
Call = Expr '(' (Expr (',' Expr)*)? ')'
MethodCall = Expr '.' Ident '(' (Expr (',' Expr)*)? ')'
```

```roca
const result = add(1, 2)
const upper = name.toUpperCase()
const item = list.get(0)
const chained = text.trim().toLowerCase()
```

### 2.5.6 Field Access

Accesses a field on a struct instance or module.

```text
FieldAccess = Expr '.' Ident
```

```roca
const name = user.name
const len = items.length
const nested = response.body.data
```

### 2.5.7 Index Access

Accesses an element by numeric index.

```text
Index = Expr '[' Expr ']'
```

```roca
const first = items[0]
const char = name[i]
const nested = matrix[row][col]
```

### 2.5.8 Match Expression

A match expression selects a branch based on a value. Every match MUST include a wildcard (`_`) arm or be exhaustive over all enum variants. Arms are separated by whitespace (newlines). Commas between arms are OPTIONAL.

```text
Match = 'match' Expr '{' MatchArm+ '}'
MatchArm = MatchPattern '=>' Expr ','?
MatchPattern = Literal | Ident '.' Ident ('(' Ident ')')? | '_'
```

```roca
const label = match status {
    "active" => "Currently active"
    "inactive" => "Not active"
    _ => "Unknown"
}

const message = match result {
    Result.Ok(value) => "Got: {value}"
    Result.Err(msg) => "Failed: {msg}"
    Result.Loading => "Please wait..."
}
```

### 2.5.9 Struct Literal

Creates an instance of a struct with field values. The struct name MUST start with an uppercase letter. Empty struct literals (`Name {}`) are NOT supported; at least one field is required.

```text
StructLit = UpperIdent '{' Ident ':' Expr (',' Ident ':' Expr)* '}'
```

```roca
const user = User { name: "Alice", age: 30 }
const point = Point { x: 0, y: 0 }
const email = Email { value: "test@example.com" }
```

### 2.5.10 Closure

An anonymous function expression. Closures use the `fn` keyword with an arrow expression body. Closure parameters are untyped — they do NOT have type annotations.

```text
Closure = 'fn' '(' Ident (',' Ident)* ')' '->' Expr
```

```roca
const double = fn(x) -> x * 2
const greet = fn(name) -> "hello {name}"
const items = list.map(fn(x) -> x + 1)
```

### 2.5.11 String Interpolation

String literals containing `{ident}` or `{ident.field}` are interpolated. The parser MUST decompose them into a sequence of string parts and expression parts. Interpolation supports identifiers and field access only. Arbitrary expressions (e.g., `{1 + 2}`, `{compute(x, y)}`) are NOT supported.

```roca
const greeting = "hello {name}, you are {age} years old"
const path = "/users/{id}/posts/{postId}"
const detail = "value: {obj.field}"
```

### 2.5.12 Enum Variant Access

Accesses a variant of an enum, optionally with data.

```text
EnumVariant = Ident '.' Ident
EnumVariantCall = Ident '.' Ident '(' (Expr (',' Expr)*)? ')'
```

```roca
const color = Color.Red
const result = Result.Ok("success")
const err = Result.Err("something went wrong")
```

### 2.5.13 Await Expression

Waits for an async operation to complete. Produces the resolved value.

```text
Await = 'wait' Expr
```

```roca
const data = wait fetchData(url)
const user = wait db.query("SELECT * FROM users WHERE id = {id}")
```

---

## 2.6 Match Patterns

Match patterns determine which arm of a match expression is selected.

### 2.6.1 Value Pattern

Matches a specific literal value. The matched value MUST be equal to the pattern.

```roca
match code {
    200 => "OK",
    404 => "Not Found",
    500 => "Server Error",
    _ => "Unknown"
}

match name {
    "admin" => "Administrator",
    "guest" => "Guest User",
    _ => name
}
```

### 2.6.2 Variant Pattern

Matches an enum variant, optionally binding its inner data to a name.

```text
VariantPattern = Ident '.' Ident ('(' Ident ')')?
```

```roca
match token {
    Token.Number(n) => "number: {n}",
    Token.String(s) => "string: {s}",
    Token.Bool(b) => "bool: {b}"
}

match status {
    Status.Active => "active",
    Status.Suspended => "suspended"
}
```

### 2.6.3 Wildcard Pattern

Matches any value. A wildcard arm MUST use `_` as the pattern. It SHOULD appear as the last arm.

```roca
match value {
    1 => "one",
    2 => "two",
    _ => "other"
}
```

---

## 2.7 Operator Precedence

Operators MUST be parsed according to the following precedence table, from highest (tightest binding) to lowest:

| Precedence | Category | Operators | Associativity |
|---|---|---|---|
| 1 (highest) | Unary | `!`, `-` | Right |
| 2 | Multiplicative | `*`, `/` | Left |
| 3 | Additive | `+`, `-` | Left |
| 4 | Comparison | `<`, `>`, `<=`, `>=` | Left |
| 5 | Equality | `==`, `!=` | Left |
| 6 | Logical AND | `&&` | Left |
| 7 (lowest) | Logical OR | `\|\|` | Left |

Binary operators at the same precedence level MUST associate left-to-right.

```roca
// Parsed as: ((!a) || ((b * c) + d) > e) && (f == g)
// Broken down:
//   !a                    — precedence 1 (unary)
//   b * c                 — precedence 2 (multiplicative)
//   (b * c) + d           — precedence 3 (additive)
//   ((b * c) + d) > e     — precedence 4 (comparison)
//   f == g                — precedence 5 (equality)
//   ... && ...            — precedence 6 (logical AND)
//   ... || ...            — precedence 7 (logical OR)

const result = a + b * c    // a + (b * c)
const check = x > 0 && y > 0  // (x > 0) && (y > 0)
const either = a == 1 || b == 2  // (a == 1) || (b == 2)
```

Parentheses MAY be used to override precedence:

```roca
const forced = (a + b) * c
const grouped = !(x > 0 && y > 0)
```
