# 6. AI Feedback Loop

Roca's compiler errors are designed to teach, not just reject. Every error includes three parts:

1. **What you wrote** — the exact line of code that triggered the error
2. **Why it's wrong** — which rule was violated and what it means
3. **What to write instead** — a concrete fix the AI (or human) can apply directly

This is the reinforced feedback loop. The compiler is a teacher. The proof test cycle is the classroom. Code that doesn't pass ownership checks doesn't compile — and the error message shows exactly how to fix it.

---

## 6.1 Feedback Format

Every ownership diagnostic MUST follow this structure:

```
{level}[{code}]: {short description}
  --> {file}:{line}:{col}
  |
{N} |     {the code you wrote}
  |           {underline} {what's wrong with it}
  |
  = help: {what to write instead}
  |
{N} |     {the corrected code}
  |     {markers showing the change}
  |
  = note: {why this fix works — the ownership concept}
```

Levels:
- `error` — code will not compile
- `note` — informational, code compiles but the compiler did something implicit

---

## 6.2 Error Messages

### E-OWN-001: Value created without an owner

**Triggered by:** A value expression (struct literal, function call, string) that isn't bound to a `const`.

```
error[E-OWN-001]: value created without an owner
  --> src/main.roca:3:5
  |
3 |     User { name: "Alice", age: 30 }
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ this creates a value but nothing owns it
  |
  = help: bind it with const:
  |
3 |     const user = User { name: "Alice", age: 30 }
  |     +++++++++++
  |
  = note: every value needs an owner. const creates ownership.
```

### E-OWN-002: Let cannot create a new value

**Triggered by:** A `let` binding with a value-producing expression (struct literal, function call, allocation) instead of a reference to an existing `const`.

```
error[E-OWN-002]: let cannot create a new value
  --> src/main.roca:3:5
  |
3 |     let user = User { name: "Alice", age: 30 }
  |     ^^^ let is a borrow — it must come from something a const owns
  |
  = help: use const to own the value, then let to borrow:
  |
3 |     const user = User { name: "Alice", age: 30 }
  |     ~~~~~ change let to const
  |
  = note: let borrows from const. const owns. they are different things.
```

### E-OWN-003: Must borrow before passing to `b` parameter

**Triggered by:** A `const` value passed directly to a function parameter declared `b`.

```
error[E-OWN-003]: const passed directly to borrowing parameter
  --> src/main.roca:5:13
  |
4 |     const file = open("data.txt")
5 |     process(file)
  |             ^^^^ process expects (b file) — you're passing the owner directly
  |
  = help: create a let borrow first:
  |
5 |     let file_ref = file
6 |     process(file_ref)
  |
  = note: this keeps file alive. process borrows it, you still own it.
```

### E-OWN-004: Use after move

**Triggered by:** Using a `const` value after it was passed to a function parameter declared `o`.

```
error[E-OWN-004]: use of moved value
  --> src/main.roca:6:10
  |
4 |     const file = open("data.txt")
5 |     consume(file)
  |             ---- file was moved here (consume takes o file)
6 |     read(file)
  |          ^^^^ file is gone — ownership transferred to consume at line 5
  |
  = help: if you need file after, borrow instead:
  |
5 |     let file_ref = file
6 |     process(file_ref)    // use (b file) not (o file)
  |
  = or: copy before consuming:
  |
5 |     consume(file.copy())
6 |     read(file)           // original still valid
  |
  = note: o means the function takes it. after that, it's gone from your scope.
```

### E-OWN-005: Parameter must declare `o` or `b`

**Triggered by:** A function parameter without an ownership qualifier.

```
error[E-OWN-005]: parameter has no ownership intent
  --> src/main.roca:1:15
  |
1 |     pub fn process(file: File) -> Ok
  |                    ^^^^ does this function borrow or consume file?
  |
  = help: add b to borrow (caller keeps it) or o to consume (caller loses it):
  |
1 |     pub fn process(b file: File) -> Ok    // borrow: caller keeps file
  |                    +
  |     pub fn process(o file: File) -> Ok    // consume: caller loses file
  |                    +
  |
  = note: b = read it, give it back. o = take it, it's yours now.
```

### E-OWN-006: Cannot return a borrow

**Triggered by:** A function returning a value derived from a `b` parameter without copying.

```
error[E-OWN-006]: function returns a borrowed value
  --> src/main.roca:3:12
  |
2 |     pub fn get_name(b user: User) -> String {
3 |         return user.name
  |                ^^^^^^^^^ user is borrowed — you can't give away something you don't own
  |
  = help: copy the value so you own the return:
  |
3 |         const name = user.name.copy()
4 |         return name
  |
  = note: return values are always owned. you borrowed user, so copy what you need.
```

### E-OWN-007: Implicit copy into container (note)

**Triggered by:** A borrowed value (`let`) stored into a container (array push, struct field, map set). Not an error — the compiler handles it — but the user should know.

```
note[E-OWN-007]: borrowed value copied into container
  --> src/main.roca:5:16
  |
3 |     let name = user.name
4 |     const names = []
5 |     names.push(name)
  |                ^^^^ name is borrowed — a copy was made so the array owns it
  |
  = note: containers always own their values. name was copied automatically.
  = note: to be explicit, write: names.push(name.copy())
```

### E-OWN-008: Reference cannot be stored or returned

**Triggered by:** Attempting to use a `let` type in a struct field declaration, or returning a `b`-qualified value.

```
error[E-OWN-008]: references cannot be stored in struct fields
  --> src/types.roca:3:5
  |
3 |     cached: let String
  |     ^^^^^^^^^^^^^^^^^^ struct fields must be owned, not borrowed
  |
  = help: remove let — the struct will own the value:
  |
3 |     cached: String
  |
  = note: structs always own their fields. when you construct it, the value is moved or copied in.
```

### E-OWN-009: Asymmetric branch consumption

**Triggered by:** An owned value consumed in one branch of an `if` but not the other.

```
error[E-OWN-009]: value consumed in one branch but not the other
  --> src/main.roca:4:5
  |
3 |     if condition {
4 |         send(data)       // data consumed here (send takes o data)
  |              ^^^^
5 |     } else {
6 |         // data not consumed here
  |
  = help: consume in both branches:
  |
5 |     } else {
6 |         drop(data)
7 |     }
  |
  = or: borrow instead so neither branch consumes:
  |
4 |         let d = data
5 |         send_ref(d)      // use (b data) instead of (o data)
  |
  = note: if one path consumes a value, all paths must. otherwise it's ambiguous who owns it after the if.
```

### E-OWN-010: Consumed in loop without reassignment

**Triggered by:** An owned value from an outer scope consumed inside a loop body, without being reassigned before the next iteration.

```
error[E-OWN-010]: value consumed in loop without reassignment
  --> src/main.roca:4:13
  |
2 |     const data = load()
3 |     while running {
4 |         process(data)
  |                 ^^^^ data is consumed each iteration but never recreated
  |
  = help: borrow instead:
  |
4 |         let d = data
5 |         read(d)          // use (b data) instead of (o data)
  |
  = or: recreate each iteration:
  |
3 |     while running {
4 |         const data = load()
5 |         process(data)
  |
  = note: if a loop consumes a value, it must make a new one before the next iteration.
```

---

## 6.3 Feedback Crate

The feedback logic lives in a dedicated crate that takes:

- The **error code** (E-OWN-001 through E-OWN-010)
- The **source line** that triggered the error
- The **context** (variable name, function name, parameter qualifier, line numbers)

And produces a structured diagnostic with the three-part format: what you wrote, why, what instead.

This crate is consumed by:
- The **checker** in roca-lang — emits diagnostics during static analysis
- The **LSP** — shows inline errors with fix suggestions in the editor
- The **CLI** — prints colored diagnostics to the terminal
- **AI agents** — parse the structured output to apply fixes automatically

The feedback crate does NOT make ownership decisions. It only formats the message. The checker decides, the feedback crate teaches.

---

## 6.4 Design Principle

The error messages are written for two audiences simultaneously:

1. **Humans** — the natural language explanation teaches the ownership concept
2. **AI coding agents** — the concrete "what to write instead" block is directly applicable as a code fix

This dual audience is intentional. Roca is designed to be written with AI assistance. The compiler's feedback loop is the training signal. Every ownership error is a lesson that makes the next attempt correct.
