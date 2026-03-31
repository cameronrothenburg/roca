# JS Output Tests

Tests that verify compiled JS output runs correctly against `@rocalang/runtime`.

These tests require Node.js and are NOT run by `cargo test`. Run them with:

```bash
./test.sh
```

## Moved from verify/

- `cross_module.rs` — tests cross-file imports produce working JS
- `imports.rs` — tests import resolution in compiled output

These were moved here because they require a JS runtime (Node/Bun) to execute.
Native-only tests remain in `tests/verify/`.
