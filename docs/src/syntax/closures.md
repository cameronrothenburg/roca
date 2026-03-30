# Closures

Closures use `fn` with an arrow body:

```roca
items.map(fn(x) -> x * 2)
items.filter(fn(x) -> x > 5)
```

Closures can be assigned to variables:

```roca
const double = fn(x) -> x * 2
```

## In crash blocks

Closures are used with `fallback` to access the error:

```roca
crash {
    Email.validate -> fallback(fn(e) -> Response.fail(400, e.message))
}
```

The closure receives an error object with `.name` and `.message` fields.
