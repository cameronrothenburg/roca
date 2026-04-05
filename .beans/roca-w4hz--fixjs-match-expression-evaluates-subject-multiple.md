---
# roca-w4hz
title: 'fix(js): match expression evaluates subject multiple times'
status: completed
type: bug
priority: critical
created_at: 2026-04-05T19:03:36Z
updated_at: 2026-04-05T19:24:00Z
---

GitHub issue #118

build_match evaluates the match subject via build_expr for each arm. Side-effectful subjects are evaluated multiple times. Fix: bind subject to a temp variable first.

## Summary of Changes

**Fix:** Match expressions now bind the subject to a temp variable `_m` via an IIFE before evaluating arms. Previously, `build_expr(ast, value)` was called per arm, duplicating the subject expression in emitted JS.

**Before:** `n === 1 ? "one" : n === 2 ? "two" : "other"` (subject evaluated per arm)
**After:** `(() => { const _m = n; return _m === 1 ? "one" : _m === 2 ? "two" : "other"; })()` (subject evaluated once)

**Files changed:**
- `crates/roca-js/src/lib.rs` — rewrote `build_match()` to use IIFE with temp binding
- `crates/roca-js/src/tests.rs` — added `match_subject_evaluated_once` test
