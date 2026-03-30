# Roca -- Grammar reference

Complete syntax for every construct in the language.

---

## Source file

A file is a sequence of top-level items:

```
SourceFile = Item*

Item = Import
     | [pub] Contract
     | [pub] Struct
     | [pub] Function
     | Satisfies
```

---

## Imports

```
Import = "import" "{" Ident ("," Ident)* "}" "from" ImportSource

ImportSource = StringLit           // "./file.roca"
             | "std"               // standard library root
             | "std" "::" Ident    // standard library module
```

```roca
import { Email, User } from "./models.roca"
import { Array } from std
import { Json } from std::json
```

---

## Contracts

```
Contract = "contract" Ident "{" ContractBody "}"

ContractBody = (Field | FnSignature | ErrDecl | EnumValue)* [MockBlock]

FnSignature = Ident "(" Params ")" "->" TypeRef ["," "err" "{" ErrDecl* "}"]

ErrDecl = "err" Ident "=" StringLit

EnumValue = NumberLit | StringLit
```

```roca
contract Stringable {
    to_string() -> String
}

contract HttpClient {
    get(url: String) -> Response, err {
        err timeout = "request timed out"
        err not_found = "404 not found"
    }

    mock {
        get -> Response {
            status: StatusCode.200
            body: Body.validate("{}")
        }
    }
}

contract StatusCode {
    200
    201
    400
    404
    500
}
```

---

## Structs

```
Struct = "struct" Ident "{" ContractBlock "}" "{" ImplBlock "}"

ContractBlock = (Field | FnSignature)*

Field = Ident ":" TypeRef [FieldConstraints]

FieldConstraints = "{" Constraint ("," Constraint)* "}"

Constraint = "min" ":" NumberLit
           | "max" ":" NumberLit
           | "minLen" ":" NumberLit
           | "maxLen" ":" NumberLit
           | "contains" ":" StringLit
           | "pattern" ":" StringLit

ImplBlock = FnDef*
```

Two blocks. First defines the contract (fields + signatures). Second provides implementations.

Fields can have inline constraints after the type:

```roca
pub struct UserProfile {
    name: String { min: 1, max: 64 }
    email: String { contains: "@", min: 3 }
    age: Number { min: 0, max: 150 }
    bio: String
}{}
```

Compiler rejects `min > max`, `contains`/`pattern` on `Number`, and any constraint on `Bool`.

```roca
pub struct Email {
    value: String

    validate(raw: String) -> Email, err {
        err missing = "value is required"
        err invalid = "format is not valid"
    }
}{
    fn validate(raw: String) -> Email, err {
        if raw.len() == 0 { return err.missing }
        if !raw.contains("@") { return err.invalid }
        return Email { value: raw }

        crash { raw.len -> halt  raw.contains -> halt }
        test {
            self("") is err.missing
            self("nope") is err.invalid
            self("a@b.com") is Ok
        }
    }
}
```

---

## Satisfies

```
Satisfies = Ident "satisfies" Ident "{" FnDef* "}"
```

Links a struct to a contract. One contract per block. No chaining.

```roca
Email satisfies String {
    fn to_string() -> String {
        return self.value
        test { self() == "a@b.com" }
    }
}
```

---

## Functions

```
FnDef = "fn" Ident "(" Params ")" ["->" TypeRef ["," "err"]] "{" Body "}"

Params = (Param ("," Param)*)?

Param = Ident ":" TypeRef

Body = Stmt* [CrashBlock] [TestBlock]
```

```roca
pub fn greet(name: String) -> String {
    let trimmed = name.trim()
    return "Hello {trimmed}"

    crash { name.trim -> halt }
    test { self("cam") == "Hello cam" }
}

fn add(a: Number, b: Number) -> Number {
    return a + b
    test {
        self(1, 2) == 3
        self(0, 0) == 0
    }
}
```

---

## Types

```
TypeRef = "String"
        | "Number"
        | "Bool"
        | "Ok"
        | Ident                         // named type: Email, User, Response
        | Ident "<" TypeRef ("," TypeRef)* ">"   // generic: Array<String>, Map<String, Number>
        | TypeRef "|" "null"            // nullable: String | null
```

```roca
name: String
count: Number
active: Bool
email: Email
items: Array<String>
scores: Map<String, Number>
nickname: String | null
nested: Array<Map<String, Number>>
```

---

## Statements

### const

```
ConstStmt = "const" Ident [":" TypeRef] "=" Expr
```

```roca
const limit = 100
const name: String = "cam"
```

### let

```
LetStmt = "let" Ident [":" TypeRef] "=" Expr
```

```roca
let count = 0
let tag: String = "default"
```

### let with destructuring (result)

```
LetResult = "let" Ident "," Ident "=" Expr
```

```roca
let email, err = Email.validate(raw)
```

### Assignment

```
Assign = Ident "=" Expr
```

```roca
count = count + 1
```

### return

```
ReturnStmt = "return" Expr
           | "return" "err" "." Ident
```

```roca
return user
return err.missing
```

### if / else

```
IfStmt = "if" Expr "{" Stmt* "}" ["else" "{" Stmt* "}"]
```

```roca
if x > 0 {
    return "positive"
} else {
    return "not positive"
}
```

### for..in

```
ForStmt = "for" Ident "in" Expr "{" Stmt* "}"
```

```roca
for item in items {
    log(item.to_string())
}
```

### while / break / continue

```
WhileStmt = "while" Expr "{" Stmt* "}"
BreakStmt = "break"
ContinueStmt = "continue"
```

```roca
let attempts = 0
while attempts < 3 {
    let result, err = try_connect()
    if err == null { break }
    attempts = attempts + 1
    continue
}
```

### wait (transparent async)

```
WaitStmt = "let" Ident "," Ident "=" "wait" Expr
WaitAllStmt = "let" Ident ("," Ident)* "," Ident "=" "waitAll" "{" Expr* "}"
WaitFirstStmt = "let" Ident "," Ident "=" "waitFirst" "{" Expr* "}"
```

```roca
// Single async call
let data, err = wait http.get(url)

// Parallel -- all must succeed
let users, prices, failed = waitAll {
    db.getUsers()
    api.getPrices()
}

// Race -- first to resolve wins
let fastest, failed = waitFirst {
    cache.get(key)
    db.get(key)
}
```

### Expression statement

Any expression on its own line:

```roca
db.save(user)
log("done")
```

---

## Expressions

### Literals

```
StringLit = '"' chars '"' | "'" chars "'" | '`' chars '`'
NumberLit = digits ["." digits]
BoolLit = "true" | "false"
NullLit = "null"
```

```roca
"hello"
42
3.14
true
false
null
```

### String interpolation

Expressions inside `{}` within strings are evaluated:

```
StringInterp = '"' (chars | "{" Expr "}")* '"'
```

```roca
"Hello {name}"
"User {user.name} is {user.age} years old"
"Total: {price.to_string()}"
```

### Identifiers and self

```roca
name
user
self            // reference to the current struct instance
self.value      // field on self
```

### Binary operators

```
Expr = Expr Op Expr

Op = "+" | "-" | "*" | "/"         // arithmetic
   | "==" | "!=" | "<" | ">"      // comparison
   | "<=" | ">="                   // comparison
   | "&&" | "||"                   // logical
```

Precedence (high to low):

| Level | Operators |
|---|---|
| 1 | `!` (unary not), `-` (unary minus) |
| 2 | `*`, `/` |
| 3 | `+`, `-` |
| 4 | `<`, `>`, `<=`, `>=` |
| 5 | `==`, `!=` |
| 6 | `&&` |
| 7 | `||` |

### Unary operators

```roca
!active
-count
```

### Parenthesized expressions

```roca
(a + b) * c
```

### Field access

```
FieldAccess = Expr "." Ident
```

```roca
user.name
user.email.value
```

### Method calls

```
MethodCall = Expr "." Ident "(" Args ")"
```

```roca
name.trim()
name.trim().to_upper()
raw.contains("@")
```

### Function calls

```
Call = Expr "(" Args ")"

Args = (Expr ("," Expr)*)?
```

```roca
greet("cam")
Email.validate(raw)
Number("42")
String(42)
Bool("true")
```

### Struct literals

```
StructLit = Ident "{" (Ident ":" Expr ",")* "}"
```

```roca
Email { value: "cam@test.com" }
User { name: "cam", email: e, age: 25 }
```

### Array literals

```
ArrayLit = "[" (Expr ("," Expr)*)? "]"
```

```roca
[1, 2, 3]
["cam", "alex"]
[]
```

### Index access

```
IndexAccess = Expr "[" Expr "]"
```

```roca
items[0]
scores["cam"]
matrix[i]
```

### Match expressions

```
MatchExpr = "match" Expr "{" MatchArm* "}"
MatchArm = (Expr | "_") "=>" Expr
```

```roca
let label = match status {
    StatusCode.200 => "ok"
    StatusCode.404 => "not found"
    _ => "unknown"
}
```

### Closures

```
Closure = "fn" "(" Ident ("," Ident)* ")" "->" Expr
```

```roca
fn(x) -> x + 1
fn(a, b) -> a + b
let doubled = items.map(fn(x) -> x * 2)
```

### Error references

```
ErrRef = "err" "." Ident
```

```roca
return err.missing
return err.invalid
```

`err.name` accesses the name string of an error at runtime.

---

## Crash blocks

```
CrashBlock = "crash" "{" CrashHandler* "}"

CrashHandler = Target "->" Strategy
             | Target "{" (ErrRef "->" Strategy)* ["default" "->" Strategy] "}"

Target = Ident ("." Ident)*

Strategy = "halt"
         | "skip"
         | "retry" "(" NumberLit "," NumberLit ")"
         | "fallback" "(" Expr ")"
```

```roca
crash {
    // Simple -- one strategy for all errors
    Email.validate -> halt

    // Per-error strategies
    db.save {
        err.timeout -> retry(3, 1000)
        err.duplicate -> skip
        default -> halt
    }
}
```

| Strategy | Behavior |
|---|---|
| `halt` | Propagate error to caller |
| `skip` | Ignore failure, continue |
| `retry(n, ms)` | Retry n times, wait ms between |
| `fallback(val)` | Use default value |

---

## Test blocks

```
TestBlock = "test" "{" TestAssertion* "}"

TestAssertion = Expr "==" Expr          // equality check
              | Expr "is" "Ok"          // success check
              | Expr "is" ErrRef        // specific error check
              | Expr "is" "err"         // any error check
```

```roca
test {
    self(1, 2) == 3
    self("a@b.com") is Ok
    self("") is err.missing
    self("nope") is err.invalid
}
```

Handler tests with mock setups:

```roca
test {
    StatusCode.200 {
        mock req.body -> Body.validate('{"name": "cam"}')
    }
    StatusCode.400 {
        mock req.body -> Body.validate('invalid')
    }
}
```

---

## Mock blocks

```
MockBlock = "mock" "{" MockEntry* "}"

MockEntry = Ident "->" Expr
          | Ident "->" Ident "{" (Ident ":" Expr)* "}"
```

```roca
mock {
    save -> Ok
    get -> Response {
        status: StatusCode.200
        body: Body.validate("{}")
    }
}
```

---

## Comments

```
Comment = "//" chars-to-end-of-line
```

```roca
// This is a comment
let x = 1  // inline comment
```

No block comments.

---

## Keywords

Reserved words that cannot be used as identifiers:

```
contract  struct  satisfies  fn  pub
const  let  return  if  else  for  in  match  while  break  continue
crash  test  mock
err  Ok  null
wait  waitAll  waitFirst
retry  skip  halt  fallback  default
import  from  std
self  is  true  false
```

---

## Punctuation and delimiters

| Token | Name |
|---|---|
| `->` | Arrow (return type, crash strategy, mock value) |
| `=>` | Fat arrow (match arms) |
| `::` | Path separator (std::module) |
| `\|` | Pipe (nullable types: Type \| null) |
| `.` | Dot (field access, method calls) |
| `,` | Comma (parameter lists, arguments) |
| `:` | Colon (type annotations, struct fields) |
| `(` `)` | Parentheses |
| `{` `}` | Braces |
| `[` `]` | Brackets |
