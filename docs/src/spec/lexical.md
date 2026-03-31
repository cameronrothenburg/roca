# 1. Lexical Grammar

This section defines the lexical structure of Roca source files. A conforming tokenizer MUST produce the token stream described here before any parsing occurs.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

---

## 1.1 Source Encoding

Roca source files MUST be encoded as UTF-8. A conforming implementation MUST reject source files that contain invalid UTF-8 byte sequences.

Source files SHOULD use the `.roca` file extension.

```
// Valid: UTF-8 encoded source
pub fn greet(name: String) -> String {
    return "hello {name}"
}
```

---

## 1.2 Whitespace

Whitespace characters (spaces `U+0020`, horizontal tabs `U+0009`, and newline characters `U+000A`) are insignificant except as token separators. A conforming tokenizer MUST consume whitespace between tokens and MUST NOT produce whitespace tokens.

Carriage return (`U+000D`) MAY appear before a newline and MUST be treated as whitespace.

Newlines MUST be tracked by the tokenizer to report accurate line numbers in diagnostics but MUST NOT produce tokens.

```roca
// These are equivalent:
pub fn add(a: Number, b: Number) -> Number { return a + b }

pub fn add(
    a: Number,
    b: Number
) -> Number {
    return a + b
}
```

---

## 1.3 Comments

Roca supports four comment forms. Regular comments are discarded during tokenization. Doc comments produce tokens that attach to the next declaration.

### 1.3.1 Line Comments

A line comment begins with `//` and extends to the end of the line. The tokenizer MUST NOT produce a token for line comments.

```roca
// This is a line comment
pub fn add(a: Number, b: Number) -> Number {
    return a + b // inline comment
}
```

### 1.3.2 Block Comments

A block comment begins with `/*` and ends with `*/`. Block comments MUST NOT nest. The tokenizer MUST NOT produce a token for block comments.

```roca
/* This is a block comment */
pub fn add(a: Number, b: Number) -> Number {
    return a + b
}

/*
  Multi-line block comments
  are also valid.
*/
```

### 1.3.3 Doc Line Comments

A doc line comment begins with `///` (three forward slashes) and extends to the end of the line. The tokenizer MUST produce a `DocComment` token containing the trimmed text content (leading whitespace after `///` is stripped, trailing whitespace is stripped).

```roca
/// Adds two numbers together.
pub fn add(a: Number, b: Number) -> Number {
    return a + b
}
```

### 1.3.4 Doc Block Comments

A doc block comment begins with `/**` and ends with `*/`. The tokenizer MUST produce a single `DocComment` token. Lines MUST be trimmed, leading `*` characters on each line MUST be stripped, and empty lines MUST be removed.

```roca
/**
 * Clamps a value between a minimum and maximum.
 * Returns the bounded result.
 */
pub fn clamp(value: Number, low: Number, high: Number) -> Number {
    return value
}
```

### 1.3.5 Disambiguation

When the tokenizer encounters `/`, it MUST apply the following rules in order:

1. If the next two characters are `//`, it is a doc line comment. Produce a `DocComment` token.
2. If the next two characters are `/*` followed by `*`, it is a doc block comment. Produce a `DocComment` token.
3. If the next character is `*`, it is a block comment. Discard.
4. If the next character is `/`, it is a line comment. Discard.
5. Otherwise, it is a `Slash` (division) operator token.

---

## 1.4 Keywords

The following 36 words are reserved keywords. A conforming tokenizer MUST emit the corresponding keyword token when any of these words appear as an identifier. Keywords are case-sensitive and MUST match exactly.

| Category | Keywords |
|---|---|
| Declarations | `contract`, `struct`, `enum`, `extern`, `satisfies`, `fn`, `pub` |
| Bindings | `const`, `let` |
| Control flow | `return`, `if`, `else`, `for`, `in`, `match`, `while`, `break`, `continue` |
| Blocks | `crash`, `test` |
| Error handling | `err`, `Ok`, `null` |
| Crash strategies | `retry`, `skip`, `halt`, `fallback`, `panic`, `default` |
| Async | `wait`, `waitAll`, `waitFirst` |
| Modules | `import`, `from`, `std` |
| Identity | `self`, `is` |
| Literals | `true`, `false` |

```roca
// Each of these produces a keyword token, not an identifier:
contract Addable { }
pub struct Point { }
enum Color { Red = "red" }
```

Note: `Ok` is the only keyword that begins with an uppercase letter. `waitAll` and `waitFirst` use camelCase. All other keywords are lowercase.

---

## 1.5 Identifiers

An identifier begins with an ASCII letter or underscore and continues with ASCII letters, digits, or underscores.

```
identifier = [a-zA-Z_][a-zA-Z0-9_]*
```

A conforming tokenizer MUST first check whether a word matches a keyword (section 1.4). If it does, the tokenizer MUST emit the keyword token. Otherwise, the tokenizer MUST emit an `Ident` token containing the matched string.

```roca
// "greet" is an identifier; "fn", "pub", "return" are keywords
pub fn greet(name: String) -> String {
    return "hello {name}"
}
```

### 1.5.1 Case Conventions

The following case conventions SHOULD be followed. A conforming compiler MAY produce warnings for violations but MUST NOT reject non-conforming names.

| Convention | Usage | Example |
|---|---|---|
| `camelCase` | Functions, methods, local bindings | `getUserName`, `totalCount` |
| `PascalCase` | Types: structs, contracts, enums, type parameters | `HttpResponse`, `Stringable` |
| `lowercase_snake` | Error names (after `err.`) | `err.not_found`, `err.invalid_email` |

```roca
// PascalCase for types
pub struct UserProfile { }

// camelCase for functions
pub fn getUserProfile(id: String) -> UserProfile {
    return UserProfile { }
}

// lowercase_snake for errors
pub fn validate(input: String) -> String, err {
    err invalid_format = "input is not valid"
    return input
}
```

---

## 1.6 Literals

### 1.6.1 Number Literals

A number literal is a sequence of one or more ASCII digits, optionally followed by a decimal point and one or more ASCII digits. All numbers MUST be stored as 64-bit floating point (`f64`).

```
integer = [0-9]+
float   = [0-9]+ '.' [0-9]+
```

A decimal point MUST only be consumed if it is immediately followed by a digit. This allows method calls on integer literals (e.g., `42.to_string()`).

```roca
const count = 42          // integer literal -> f64
const pi = 3.14159        // float literal -> f64
const label = 42.to_string()  // 42 is a number, .to_string() is a method call
```

### 1.6.2 String Literals

String literals are delimited by double quotes (`"`), single quotes (`'`), or backticks (`` ` ``). All three forms produce the same `StringLit` token. Backtick strings MAY span multiple lines.

```roca
const a = "hello world"
const b = 'hello world'
const c = `hello
world`
```

### 1.6.3 String Escape Sequences

Within any string literal, the following escape sequences MUST be recognized:

| Sequence | Meaning |
|---|---|
| `\n` | Newline (U+000A) |
| `\t` | Horizontal tab (U+0009) |
| `\\` | Backslash (U+005C) |
| `\"` | Double quote (U+0022) |
| `\'` | Single quote (U+0027) |

An unrecognized escape (e.g., `\z`) MUST be preserved literally as the two characters `\z`.

```roca
const line = "first\nsecond"     // contains a newline
const tab = "col1\tcol2"         // contains a tab
const escaped = "she said \"hi\""  // contains literal double quotes
```

### 1.6.4 String Interpolation

Curly braces `{` and `}` inside string literals denote interpolation. Interpolation supports identifiers and single-level field access only. Arbitrary expressions are NOT supported.

```roca
const name = "world"
const greeting = "hello {name}"                      // "hello world"
const info = "{user.name} is {user.age} years old"   // field access
```

The following are NOT valid interpolation and will be treated as literal text:

```roca
const bad = "result: {1 + 2}"          // NOT interpolated — literal text
const bad2 = "len: {name.length()}"    // NOT interpolated — method calls not supported
```

### 1.6.5 Boolean Literals

The keywords `true` and `false` MUST be tokenized as `BoolLit(true)` and `BoolLit(false)` respectively. They are not identifiers.

```roca
const enabled = true
const verbose = false
```

### 1.6.6 Null Literal

The keyword `null` MUST be tokenized as a `Null` token. It exists for interop with external APIs that return null values. Roca code SHOULD use `Optional<T>` for absence and `-> T, err` for failures instead of null.

---

## 1.7 Operators

Operators are one or two characters that MUST be tokenized as described below. Two-character operators MUST be matched before single-character operators (longest match rule).

### 1.7.1 Arithmetic Operators

| Token | Symbol | Description |
|---|---|---|
| `Plus` | `+` | Addition |
| `Minus` | `-` | Subtraction |
| `Star` | `*` | Multiplication |
| `Slash` | `/` | Division |

```roca
const sum = a + b
const diff = a - b
const product = a * b
const quotient = a / b
```

### 1.7.2 Comparison Operators

| Token | Symbol | Description |
|---|---|---|
| `Eq` | `==` | Equality |
| `Neq` | `!=` | Inequality |
| `Lt` | `<` | Less than |
| `Gt` | `>` | Greater than |
| `Lte` | `<=` | Less than or equal |
| `Gte` | `>=` | Greater than or equal |

```roca
if a == b { }
if a != b { }
if a < b { }
if a >= b { }
```

### 1.7.3 Logical Operators

| Token | Symbol | Description |
|---|---|---|
| `And` | `&&` | Logical AND |
| `Or` | `\|\|` | Logical OR |
| `Not` | `!` | Logical NOT (unary) |

```roca
if a > 0 && b > 0 { }
if a == 0 || b == 0 { }
if !enabled { }
```

### 1.7.4 Assignment Operator

| Token | Symbol | Description |
|---|---|---|
| `Assign` | `=` | Assignment |

```roca
let count = 0
count = count + 1
```

### 1.7.5 Arrow Operators

| Token | Symbol | Description |
|---|---|---|
| `Arrow` | `->` | Return type annotation, crash handler mapping |
| `FatArrow` | `=>` | Match arm separator |

```roca
// Arrow: return type
pub fn add(a: Number, b: Number) -> Number {
    return a + b
}

// Arrow: crash handler
crash {
    http.get -> retry(3, 1000)
}

// Fat arrow: match arm
match color {
    Color.Red => "red"
    _ => "unknown"
}
```

### 1.7.6 Pipe Operator

| Token | Symbol | Description |
|---|---|---|
| `PipeArrow` | `\|>` | Crash chain (pipes result through error-handling stages) |

```roca
crash {
    db.query -> retry(3) |> fallback(cachedResult)
}
```

### 1.7.7 Union Operator

| Token | Symbol | Description |
|---|---|---|
| `Pipe` | `\|` | Nullable type union |

```roca
pub fn find(id: String) -> User | null {
    return null
}
```

### 1.7.8 Module Path Operator

| Token | Symbol | Description |
|---|---|---|
| `ColonColon` | `::` | Module path separator |

```roca
import { readFile } from std::fs
import { HttpClient } from std::http
```

### 1.7.9 Is Operator

| Token | Symbol | Description |
|---|---|---|
| `Is` | `is` | Test assertion for error identity |

The `is` keyword MUST be tokenized as an `Is` token. It is used in test blocks to assert that a call produces a specific error.

```roca
test {
    self("") is err.invalid_input
}
```

---

## 1.8 Punctuation

### 1.8.1 Delimiters

| Token | Symbol | Description |
|---|---|---|
| `LParen` / `RParen` | `(` `)` | Function parameters, call arguments, grouping |
| `LBrace` / `RBrace` | `{` `}` | Blocks, struct literals, contracts, enums |
| `LBracket` / `RBracket` | `[` `]` | Array literals, index access |

```roca
pub fn process(items: [String]) -> Number {
    const result = items[0]
    return result.length
}
```

### 1.8.2 Separators

| Token | Symbol | Description |
|---|---|---|
| `Dot` | `.` | Field access, method call, error reference |
| `Comma` | `,` | Parameter separator, argument separator, error flag |
| `Colon` | `:` | Type annotation |
| `Semicolon` | `;` | Statement terminator (OPTIONAL) |

```roca
pub fn example(name: String, age: Number) -> String, err {
    err too_young = "must be 18 or older"
    const greeting = "hello {name}"
    return greeting
}
```

---

## 1.9 Token Ordering

A conforming tokenizer MUST attempt to match tokens in the following order at each position:

1. Newlines (consumed for line tracking, no token emitted)
2. Whitespace (consumed, no token emitted)
3. Doc line comments (`///`)
4. Doc block comments (`/**`)
5. Block comments (`/*`)
6. Line comments (`//`)
7. String literals (`"`, `'`, `` ` ``)
8. Number literals (`[0-9]`)
9. Identifiers and keywords (`[a-zA-Z_]`)
10. Two-character operators (`::`, `->`, `=>`, `==`, `!=`, `<=`, `>=`, `&&`, `||`, `|>`)
11. Single-character operators and punctuation

An `EOF` token MUST be appended after all source characters have been consumed.

Characters that do not match any rule MUST be silently skipped. The parser is responsible for reporting structural errors.
