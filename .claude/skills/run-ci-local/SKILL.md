---
name: run-ci-local
description: Run the full CI pipeline locally before pushing. Builds, runs Rust tests, smoke tests, and JS integration tests.
disable-model-invocation: true
---

# Run CI Locally

Run the same steps as `.github/workflows/ci.yml` to catch failures before pushing.

## Steps

Run these sequentially — each step must pass before proceeding:

```bash
# 1. Build release binary
cargo build --release

# 2. Run all Rust tests
cargo test --release

# 3. CLI smoke test
./target/release/roca check tests/js/projects/api

# 4. JS integration tests
cd tests/js && bun install && ROCA_BIN=../../target/release/roca bun test
```

## On failure

- **Build fails**: Check compiler errors, fix, re-run
- **Rust tests fail**: Run the specific failing test with `cargo test --release test_name -- --nocapture`
- **Smoke test fails**: Run `roca check` on the failing project path for diagnostics
- **JS tests fail**: Run specific test file with `ROCA_BIN=../../target/release/roca bun test filename.test.js`

## Report

After running, report:
- Which steps passed/failed
- For failures: the specific test name and error output
- Suggested fix if the error is obvious
