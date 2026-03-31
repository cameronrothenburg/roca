# 3. Type System

**Status:** Draft

This section defines the type system of the Roca language, including primitive types, type references, contracts, structs, enums, generics, and nullable types.

---

## 3.1 Primitive Types

Roca provides four primitive types. All primitives are immutable value types.

### 3.1.1 `String`

A `String` value MUST be valid UTF-8 text. String literals are written with double quotes.

**Methods:**

| Method | Signature | Description |
|--------|-----------|-------------|
| `trim` | `() -> String` | Remove leading and trailing whitespace |
| `toUpperCase` | `() -> String` | Convert all characters to uppercase |
| `toLowerCase` | `() -> String` | Convert all characters to lowercase |
| `slice` | `(start: Number, end: Number) -> String` | Extract substring by index range |
| `includes` | `(search: String) -> Bool` | Test whether string contains substring |
| `startsWith` | `(prefix: String) -> Bool` | Test whether string starts with prefix |
| `endsWith` | `(suffix: String) -> Bool` | Test whether string ends with suffix |
| `indexOf` | `(search: String) -> Number` | Return index of first occurrence, or -1 |
| `split` | `(delimiter: String) -> Array<String>` | Split string into array by delimiter |
| `charAt` | `(index: Number) -> String` | Return character at index |
| `charCodeAt` | `(index: Number) -> Number` | Return Unicode code point at index |
| `replace` | `(search: String, replacement: String) -> String` | Replace first occurrence |
| `repeat` | `(count: Number) -> String` | Repeat string count times |
| `length` | `Number` | Number of characters (property, not method) |
| `toString` | `() -> String` | Identity — returns self |

### 3.1.2 `Number`

A `Number` value MUST be an IEEE 754 double-precision floating-point number.

**Methods:**

| Method | Signature | Description |
|--------|-----------|-------------|
| `toString` | `() -> String` | Convert to string representation |
| `toFixed` | `(digits: Number) -> String` | Format with fixed decimal places |

### 3.1.3 `Bool`

A `Bool` value MUST be either `true` or `false`.

**Methods:**

| Method | Signature | Description |
|--------|-----------|-------------|
| `toString` | `() -> String` | Returns `"true"` or `"false"` |

### 3.1.4 `Ok`

`Ok` is the unit return type for functions that succeed without producing a value. A function with return type `Ok` MUST NOT return data — it signals successful completion only.

```roca
pub fn log(msg: String) -> Ok {
    Console.print(msg)
    return Ok
test {
    self("hello") is Ok
}
}
```

---

## 3.2 Type References

A type reference (TypeRef) identifies the type of a value, parameter, field, or return position. The grammar of type references is:

```
TypeRef = String | Number | Bool | Ok
        | Named(name)                    // User-defined: Email, ApiResponse
        | Generic(name, args)            // Array<String>, Map<Number>
        | Nullable(inner)                // Type | null
        | Fn(params, return)             // fn(String) -> Number
```

### 3.2.1 Primitive References

The identifiers `String`, `Number`, `Bool`, and `Ok` are reserved type names. A conforming implementation MUST NOT allow user-defined types with these names.

### 3.2.2 Named References

A named reference refers to a user-defined type — a struct, enum, or contract.

```roca
const email: Email = Email.validate("a@b.com")
```

### 3.2.3 Generic References

A generic reference applies one or more type arguments to a parameterized type.

```roca
const items: Array<String> = ["one", "two", "three"]
const lookup: Map<String, Number> = Map.from([["a", 1], ["b", 2]])
```

### 3.2.4 Nullable References

A nullable reference is written as `Type | null`. See [Section 3.10](#310-nullable-types).

### 3.2.5 Function References

A function type reference describes a callable value. The syntax is `fn(ParamTypes) -> ReturnType`.

```roca
const transform: fn(String) -> Number = fn(s) -> s.length
```

---

## 3.3 Contracts

A contract defines what a type can do. Contracts are analogous to interfaces or traits — they declare method signatures without implementations.

### 3.3.1 Internal Contracts

An internal contract defines a type shape that Roca structs can satisfy.

```roca
contract Loggable {
    toLog() -> String
}
```

- A contract body MUST contain one or more method signatures.
- Method signatures MUST NOT include implementations.
- Contract names MUST start with an uppercase letter.

### 3.3.2 Extern Contracts

An extern contract declares a type whose runtime implementation is provided externally (by JavaScript or native code). The `pub extern` modifier marks the contract as externally implemented.

```roca
pub extern contract Http {
    get(url: String) -> HttpResponse, err {
        err network = "network error"
        err timeout = "request timed out"
    }
    post(url: String, body: String) -> HttpResponse, err {
        err network = "network error"
    }
}
```

- Extern contracts MUST be marked `pub extern contract`.
- Methods on extern contracts MAY include error declarations (`, err { ... }`).
- Error declarations on extern contract methods define the error names that crash blocks MUST handle.
- The runtime binding for an extern contract is target-specific and outside the scope of this specification.

### 3.3.3 Contract Method Signatures

A method signature within a contract declares the method name, parameter types, and return type.

```
MethodSig = name "(" Params ")" "->" TypeRef
           | name "(" Params ")" "->" TypeRef "," "err" ErrorDecl?
```

- Parameters MUST be fully typed.
- A method signature MAY include `, err` to indicate the method can fail.
- Error declarations (`err name = "message"`) MAY follow the `, err` marker inside braces.

---

## 3.4 Structs

A struct is a concrete type with named fields, optional contract signatures, and method implementations. Structs are the primary way to define data types in Roca.

### 3.4.1 Struct Syntax

A struct declaration has two brace-delimited blocks:

1. **Header block** — fields and contract method signatures.
2. **Implementation block** — method bodies.

```roca
pub struct Email {
    value: String { contains: "@", minLen: 5 }
    validate(raw: String) -> Email, err { err invalid = "invalid email" }
}{
    pub fn validate(raw: String) -> Email, err {
        return Email { value: raw }
    test {
        self("a@b.com") is Ok
        self("bad") is err.invalid
    }
    }
}
```

### 3.4.2 Header Block

The header block declares:

- **Fields:** Named, typed values stored in the struct. Fields MAY have [constraints](#35-field-constraints).
- **Method signatures:** Declare callable methods, optionally with error declarations.

A struct MUST have at least one field or one method signature.

### 3.4.3 Implementation Block

The implementation block contains method bodies. Each method:

- MUST match a signature declared in the header block.
- MUST include a `test` block (see [Section 6](./testing.md)).
- MAY be marked `pub` for external visibility.

### 3.4.4 Struct Literals

A struct value is constructed with a struct literal:

```roca
const e = Email { value: "hello@example.com" }
```

Field constraints (see [Section 3.5](#35-field-constraints)) are validated at construction time. If a constraint is violated, the construction MUST fail with a constraint error.

---

## 3.5 Field Constraints

Fields on structs MAY declare constraints that restrict valid values. Constraints are written inline after the field type, inside braces.

```roca
port: Number { min: 1, max: 65535, default: 8080 }
name: String { minLen: 1, maxLen: 64 }
email: String { contains: "@" }
```

### 3.5.1 Available Constraints

| Constraint | Applies to | Description |
|------------|-----------|-------------|
| `min` | `Number` | Minimum value (inclusive) |
| `max` | `Number` | Maximum value (inclusive) |
| `minLen` | `String` | Minimum character length (inclusive) |
| `maxLen` | `String` | Maximum character length (inclusive) |
| `contains` | `String` | MUST contain the given substring |
| `pattern` | `String` | MUST match the given regex pattern |
| `default` | Any | Default value when field is omitted |

### 3.5.2 Constraint Validation

- Constraints MUST be validated at construction time (struct literal creation).
- If a field has a `default` constraint and the field is omitted from the struct literal, the default value MUST be used.
- If a constraint is violated and no `default` is specified, the construction MUST return early with the default value for the type (`""` for String, `0` for Number, `false` for Bool).
- A conforming compiler SHOULD emit constraint checks as part of the struct constructor in the compilation target.

---

## 3.6 Function Parameter Constraints

Function parameters MAY use the same constraint syntax as struct fields. Constraints on parameters are validated at function entry.

```roca
pub fn clamp(n: Number { min: 0, max: 100 }) -> Number {
    return n * 2
test {
    self(50) is 100
    self(0) is 0
}
}
```

- Parameter constraints use the same syntax and same constraint names as field constraints (see [Section 3.5.1](#351-available-constraints)).
- Validation MUST occur before the function body executes.
- If a constraint is violated, the function MUST return early with the default value for the return type.

---

## 3.7 Enums

An enum defines a type with a fixed set of variants. Roca supports two enum forms: simple enums and algebraic enums.

### 3.7.1 Simple Enums

A simple enum assigns each variant a literal value (string or number).

```roca
enum Color {
    Red = "red"
    Blue = "blue"
    Green = "green"
}
```

- Each variant MUST have an explicit value assignment.
- Values MUST be string literals or number literals.
- Variant names MUST start with an uppercase letter.

### 3.7.2 Algebraic Enums

An algebraic enum defines variants that MAY carry typed data.

```roca
enum Token {
    Number(Number)
    Plus
    Minus
    Ident(String)
}
```

- **Unit variants** carry no data: `Token.Plus`, `Token.Minus`.
- **Data variants** carry one or more typed values: `Token.Number(42)`, `Token.Ident("x")`.
- A data variant is constructed by calling it with the appropriate arguments: `Token.Number(42)`.

### 3.7.3 Recursive Enums

Enum variants MAY reference the enclosing enum type, enabling recursive data structures.

```roca
enum Expr {
    Num(Number)
    Add(Expr, Expr)
}
```

A conforming implementation MUST support recursive enums. The compiler SHOULD use heap allocation for recursive variants in the native target.

---

## 3.8 Generics

Contracts and structs MAY be parameterized with type variables, enabling generic programming.

### 3.8.1 Type Parameters

Type parameters are declared in angle brackets after the type name.

```roca
contract Array<T> {
    map(callback: fn(T) -> T) -> Array<T>
    filter(callback: fn(T) -> Bool) -> Array<T>
    find(callback: fn(T) -> Bool) -> T | null
    length -> Number
}
```

- Type parameter names MUST be uppercase single letters or uppercase identifiers.
- Multiple type parameters are separated by commas: `<K, V>`.

### 3.8.2 Constrained Type Parameters

A type parameter MAY be constrained to types that satisfy a contract.

```roca
contract Sortable<T: Comparable> {
    sort() -> Array<T>
}
```

- The syntax is `<T: ContractName>`.
- A type argument passed for `T` MUST satisfy the named contract.
- A conforming compiler MUST reject type arguments that do not satisfy the constraint.

---

## 3.9 Satisfies

The `satisfies` declaration implements a contract for a struct. This is how a struct declares that it fulfills a contract's requirements.

```roca
Email satisfies Loggable {
    fn toLog() -> String {
        return self.value
    test {
        const e = Email { value: "a@b.com" }
        e.toLog() is "a@b.com"
    }
    }
}
```

### 3.9.1 Rules

- The struct MUST implement all methods defined in the contract.
- Method signatures MUST match the contract exactly — same parameter types, same return type.
- Each method implementation MUST include a `test` block.
- A struct MAY satisfy multiple contracts via separate `satisfies` declarations.
- The `satisfies` declaration MUST appear at the module level, not nested inside other declarations.

### 3.9.2 Self Reference

Inside a `satisfies` block, `self` refers to the struct instance on which the method is called.

```roca
Email satisfies Loggable {
    fn toLog() -> String {
        return self.value    // self is the Email instance
    test {
        const e = Email { value: "test@example.com" }
        e.toLog() is "test@example.com"
    }
    }
}
```

---

## 3.10 Optional Types

Roca does not have `null`. Values that may be absent use `Optional<T>`.

```roca
contract Optional<T> {
    isPresent() -> Bool
    unwrap() -> T, err {
        err absent = "value is not present"
    }
    unwrapOr(fallback: T) -> T
}
```

### 3.10.1 Usage

Optional is used for struct fields and contract return types where absence is meaningful:

```roca
// Contract method that may not find a result
find(callback: fn(T) -> Bool) -> Optional<T>

// Struct field that may be absent
pub struct UserProfile {
    bio: Optional<String>
}
```

### 3.10.2 Accessing Optional Values

Code that receives an `Optional<T>` MUST handle the absent case:

```roca
const result = items.find(fn(s) -> s == "target")
if result.isPresent() {
    const value = result.unwrap()
    log(value)
}

// Or with a fallback
const safe = result.unwrapOr("default")
```

### 3.10.3 Optional vs Error Returns

- `Optional<T>` — value may be absent (no error, just missing)
- `-> T, err` — operation can fail (error with name and message)

Functions MUST NOT use `Optional` for failure cases. Use `-> T, err` with crash blocks instead.

### 3.10.4 Null

Roca does not use `null` in user code. The `null` value exists only to represent values returned by external APIs (JS globals, extern contracts). When an extern contract method returns a value that may be `null`, the contract SHOULD declare the return type as `Optional<T>`.

```roca
pub extern contract Http {
    /// Response header — null if header not present
    header(name: String) -> Optional<String>
}
```

The compiler's internal type system has a `Nullable(Box<TypeRef>)` variant for interop, but user-facing Roca code models absence through `Optional<T>` and failure through `-> T, err`.
