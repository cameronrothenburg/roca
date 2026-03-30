# Control Flow

## If / else

```roca
if name == "" {
    return err.invalid_name
} else {
    return name.trim()
}
```

## For loops

```roca
for item in items {
    log(item)
}
```

## While loops

```roca
while condition {
    // break and continue are supported
    if done { break }
    continue
}
```

## Match

```roca
match status_code {
    200 => "ok"
    404 => err.not_found
    _ => "unknown"
}
```

The `_` arm is the default case.
