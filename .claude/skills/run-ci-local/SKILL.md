---
name: run-ci-local
description: "Run the full CI pipeline locally. TRIGGER when: verifying the whole project works, before pushing, or as a final check in any workflow. Mirrors .github/workflows/ci.yml exactly."
---

# Run CI Locally

Run the same steps as `.github/workflows/ci.yml` to catch failures before pushing. This skill should be invoked by agent teams as a final verification step.

## Steps

Run these sequentially — each step must pass before proceeding:

### 1. Build release binary

```bash
cargo build --release
```

### 2. Run all Rust tests

```bash
cargo test --workspace --release
```

### 3. CLI smoke test

```bash
./target/release/roca check tests/js/projects/api
```

### 4. JS integration tests

```bash
cd tests/js && bun install && ROCA_BIN=../../target/release/roca bun test
```

## On failure

- **Build fails**: check compiler errors, fix, re-run
- **Rust tests fail**: run the specific failing test with `cargo test --release test_name -- --nocapture`
- **Smoke test fails**: run `roca check` on the failing project path for diagnostics
- **JS tests fail**: run specific test file with `ROCA_BIN=../../target/release/roca bun test filename.test.js`

## Report

After running, report:
- Which steps passed/failed
- For failures: the specific test name and error output
- Total test counts if available
