# 2. Syntax

This section defines the syntactic structure of Roca programs. A conforming parser MUST accept programs that follow the grammar defined here and MUST reject programs that do not.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

---

## 2.1 Source File

A source file is a sequence of zero or more top-level items. A conforming parser MUST consume all tokens until `EOF` and produce a `SourceFile` node containing the ordered list of items.

```
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

```
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

An import brings names from another module into scope.

```
Import = 'import' '{' Ident (',' Ident)* '}' 'from' ImportSource
ImportSource = StringLit | 'std' ('::' Ident)?
```

The import list MUST contain at least one name. The source MUST be either a string literal (relative path) or `std` with an optional module path.

```roca
// Import from a relative file
import { UserProfile } from "./types.roca"

// Import from the standard library root
import { map } from std

// Import from a standard library module
import { readFile, writeFile } from std::fs
```

### 2.2.2 Contract

A contract declares a set of capabilities that a type must implement. It defines function signatures, fields, and optional type parameters.

```
Contract = 'pub'? 'contract' Ident TypeParams? '{' (FnSignature | Field)* '}'
TypeParams = '<' TypeParam (',' TypeParam)* '>'
TypeParam = Ident (':' Ident)?
```

The `pub` modifier is OPTIONAL. Type parameters MAY have a constraint (another contract name) separated by `:`.

```roca
contract Stringable {
    to_string() -> String
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

```
ExternContract = 'pub' 'extern' 'contract' Ident TypeParams? '{' (FnSignature | Field)* '}'
```

Extern contracts MUST use the `pub` modifier.

```roca
pub extern contract Console {
    log(message: String) -> null
    error(message: String) -> null
}
```

### 2.2.4 Enum

An enum defines a type with a fixed set of named variants. Enums come in two forms: flat (key-value) and algebraic (data-carrying).

```
Enum = 'pub'? 'enum' Ident '{' EnumVariants '}'
EnumVariants = FlatVariants | AlgebraicVariants
FlatVariants = Ident '=' (StringLit | NumberLit) (',' Ident '=' (StringLit | NumberLit))*
AlgebraicVariants = AlgebraicVariant ('|' AlgebraicVariant)*
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
    | Err(String)
    | Loading
}
```

### 2.2.5 Struct

A struct defines a named data type with fields, function signatures, and method implementations. Structs use a two-block syntax: the first block declares the contract (fields and signatures), the second block provides implementations.

```
Struct = 'pub'? 'struct' Ident '{' (Field | FnSignature)* '}' '{' FnDef* '}'
```

The first block MUST contain fields and/or function signatures. The second block MUST contain function definitions that implement the declared signatures.

```roca
pub struct Email {
    value: String { contains: "@", maxLen: 255 }

    to_string() -> String
}{
    fn to_string() -> String {
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

```
Satisfies = Ident 'satisfies' Ident TypeArgs? '{' FnDef* '}'
TypeArgs = '<' TypeRef (',' TypeRef)* '>'
```

The first identifier MUST name an existing struct. The second MUST name an existing contract.

```roca
Email satisfies Stringable {
    fn to_string() -> String {
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

```
ExternFn = 'pub' 'extern' 'fn' Ident '(' Params ')' '->' TypeRef (',' 'err')? '{' ErrDecl* '}'
```

Extern functions MUST use the `pub` modifier. They MUST NOT have a body, crash block, or test block — only error declarations.

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

A function definition is the primary executable item. It consists of a signature, a body, an optional crash block, and an optional test block.

```
FnDef = DocComment? 'pub'? 'fn' Ident TypeParams? '(' Params ')' '->' TypeRef (',' 'err')? '{'
            ErrDecl*
            Stmt*
        'crash' '{'
            CrashHandler*
        '}'
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

Parameters are a comma-separated list of name-type pairs. Each parameter MAY include constraints in curly braces after the type.

```
Params = (Param (',' Param)*)?
Param = Ident ':' TypeRef ('{' Constraint (',' Constraint)* '}')?
Constraint = Ident ':' (NumberLit | StringLit)
```

```roca
pub fn clamp(
    value: Number,
    low: Number { max: 1000 },
    high: Number { min: 0 }
) -> Number {
    return value
}
```

### 2.3.3 Return Type

Every function MUST declare a return type with `->`. If the function can return errors, the return type MUST be followed by `, err`.

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

Error declarations MUST appear at the top of the function body, before any statements. Each declares a named error with a message.

```
ErrDecl = 'err' Ident '=' StringLit
```

```roca
pub fn parse(input: String) -> Number, err {
    err empty_input = "input must not be empty"
    err not_a_number = "input is not numeric"

    // body follows error declarations
    return 0
}
```

### 2.3.5 Body

The function body contains the happy-path logic. It MUST contain only statements and expressions that handle the success case. Error conditions are signaled by returning `err.name`.

### 2.3.6 Crash Block

The crash block declares error-handling strategies for dependencies that can fail. It appears after the body, inside the function's closing brace.

```
CrashBlock = 'crash' '{' CrashHandler* '}'
CrashHandler = DottedName '->' CrashStrategy ('|>' CrashStrategy)*
CrashStrategy = ('retry' | 'skip' | 'halt' | 'fallback' | 'panic' | 'default') '(' Args ')'
```

Crash strategies MUST be one of: `retry`, `skip`, `halt`, `fallback`, `panic`, `default`.

```roca
pub fn fetchUser(id: String) -> User, err {
    err not_found = "user does not exist"
    const response = http.get("/users/{id}")
    return User { name: response.name }
crash {
    http.get -> retry(3, 1000)
    http.get -> retry(3) |> fallback(defaultUser)
}
test {
    self("abc") == User { name: "test" }
}}
```

### 2.3.7 Test Block

The test block contains proof assertions for the function. It appears after the crash block (or after the body if there is no crash block), inside the function's closing brace.

```
TestBlock = 'test' '{' TestCase* '}'
TestCase = 'self' '(' Args ')' ('==' Expr | 'is' 'err' '.' Ident)
```

Test cases call the function using `self` and assert either value equality (`==`) or error identity (`is`).

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
}}
```

### 2.3.8 Block Ordering

Within a function's braces, elements MUST appear in this order:

1. Error declarations (`err name = "message"`)
2. Body statements
3. Crash block (OPTIONAL)
4. Test block (OPTIONAL)

A function MUST NOT have a test block before a crash block.

---

## 2.4 Statements

Statements are the executable units within function bodies, control flow blocks, and struct methods.

```
Stmt = ConstDecl | LetDecl | Assignment | Return | If | While | For | Break | Continue
     | FieldAssignment | Wait | WaitAll | WaitFirst | ExprStmt
```

### 2.4.1 Const Declaration

An immutable binding. The value MUST NOT be reassigned after declaration.

```
ConstDecl = 'const' Ident '=' Expr
```

```roca
const name = "roca"
const count = items.length
const result = add(1, 2)
```

### 2.4.2 Let Declaration

A mutable binding. The value MAY be reassigned.

```
LetDecl = 'let' Ident '=' Expr
```

```roca
let count = 0
let message = "initial"
```

### 2.4.3 Assignment

Reassignment of a `let` binding. The target MUST have been declared with `let`. Assigning to a `const` binding is a compile error.

```
Assignment = Ident '=' Expr
```

```roca
let count = 0
count = count + 1
count = count * 2
```

### 2.4.4 Return

Returns a value from the enclosing function. A function body MUST contain at least one `return` statement on every code path.

```
Return = 'return' Expr
```

The expression MAY be a value or an error reference:

```roca
return 42
return "hello"
return err.not_found
return Ok(result)
```

### 2.4.5 If / Else

Conditional execution. The condition MUST be an expression. The `else` branch is OPTIONAL. `else if` chaining is permitted.

```
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

```
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

```
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

```
Break = 'break'
Continue = 'continue'
```

These statements MUST only appear inside a `while` or `for` loop body.

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

```
FieldAssignment = Expr '.' Ident '=' Expr
```

```roca
self.count = self.count + 1
user.name = newName
```

### 2.4.10 Wait, WaitAll, WaitFirst

Async operations for concurrent execution.

```
Wait = 'wait' Expr
WaitAll = 'waitAll' '{' Stmt* '}'
WaitFirst = 'waitFirst' '{' Stmt* '}'
```

`wait` suspends until a single async operation completes. `waitAll` runs all operations concurrently and waits for all to complete. `waitFirst` runs all operations concurrently and returns when the first completes.

```roca
// Single await
const response = wait http.get("/api/users")

// Concurrent — wait for all
waitAll {
    const users = http.get("/api/users")
    const posts = http.get("/api/posts")
}

// Concurrent — first to resolve wins
waitFirst {
    const primary = db.query("SELECT * FROM cache")
    const fallback = http.get("/api/data")
}
```

---

## 2.5 Expressions

Expressions produce values. They may appear on the right side of bindings, as function arguments, in conditions, and as return values.

```
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

```
Binary = Expr Op Expr
Op = '+' | '-' | '*' | '/' | '==' | '!=' | '<' | '>' | '<=' | '>=' | '&&' | '||'
```

```roca
const sum = a + b
const isValid = age >= 18 && hasPermission == true
const combined = firstName + " " + lastName
```

### 2.5.4 Unary Expressions

A unary expression applies a prefix operator to a single operand.

```
Unary = '!' Expr
```

```roca
if !isValid {
    return err.invalid
}
```

### 2.5.5 Call Expressions

A call expression invokes a function or method with arguments.

```
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

```
FieldAccess = Expr '.' Ident
```

```roca
const name = user.name
const len = items.length
const nested = response.body.data
```

### 2.5.7 Index Access

Accesses an element by numeric index.

```
Index = Expr '[' Expr ']'
```

```roca
const first = items[0]
const char = name[i]
const nested = matrix[row][col]
```

### 2.5.8 Match Expression

A match expression selects a branch based on a value. Every match MUST include a wildcard (`_`) arm or be exhaustive over all enum variants.

```
Match = 'match' Expr '{' MatchArm (',' MatchArm)* '}'
MatchArm = MatchPattern '=>' Expr
MatchPattern = Literal | Ident '.' Ident ('(' Ident ')')? | '_'
```

```roca
const label = match status {
    "active" => "Currently active",
    "inactive" => "Not active",
    _ => "Unknown"
}

const message = match result {
    Result.Ok(value) => "Got: {value}",
    Result.Err(msg) => "Failed: {msg}",
    Result.Loading => "Please wait..."
}
```

### 2.5.9 Struct Literal

Creates an instance of a struct with field values.

```
StructLit = Ident '{' (Ident ':' Expr (',' Ident ':' Expr)*)? '}'
```

```roca
const user = User { name: "Alice", age: 30 }
const point = Point { x: 0, y: 0 }
const email = Email { value: "test@example.com" }
```

### 2.5.10 Closure

An anonymous function expression. Closures use the `fn` keyword with an arrow expression body.

```
Closure = 'fn' '(' Params ')' '->' Expr
```

```roca
const double = fn(x: Number) -> x * 2
const greet = fn(name: String) -> "hello {name}"
const items = list.map(fn(x: Number) -> x + 1)
```

### 2.5.11 String Interpolation

String literals containing `{expr}` are interpolated expressions. The parser MUST decompose them into a sequence of string parts and expression parts.

```roca
const greeting = "hello {name}, you are {age} years old"
const path = "/users/{id}/posts/{postId}"
const debug = "result: {compute(x, y)}"
```

### 2.5.12 Enum Variant Access

Accesses a variant of an enum, optionally with data.

```
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

```
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

```
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
| 1 (highest) | Unary | `!` | Right |
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
