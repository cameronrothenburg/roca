# Telemetry & Observability

Every `roca build` and `roca check` logs structured events to `~/.roca/logs/roca.jsonl`. This gives you a full audit trail of compilations, errors, and test results.

## Log location

```
~/.roca/logs/roca.jsonl
```

One JSON object per line (JSONL format). Append-only — new events are added to the end.

## Events

### `parse_error`

Fired when a `.roca` file fails to parse.

```json
{
  "ts": "1711843200",
  "event": "parse_error",
  "file": "src/validate.roca",
  "message": "expected identifier, got LBrace",
  "source": "pub fn validate(...) {\n..."
}
```

### `check_errors`

Fired when the type checker finds rule violations.

```json
{
  "ts": "1711843200",
  "event": "check_errors",
  "file": "src/validate.roca",
  "error_count": 2,
  "errors": [
    { "code": "missing-crash", "message": "'db.query' returns errors but has no crash handler", "context": "get_users" },
    { "code": "untested-error", "message": "error 'not_found' is not tested", "context": "get_users" }
  ],
  "source": "pub fn get_users(...) {\n..."
}
```

### `test_result`

Fired after proof tests run.

```json
{
  "ts": "1711843200",
  "event": "test_result",
  "file": "src/validate.roca",
  "passed": 15,
  "failed": 0,
  "output": "15 passed, 0 failed"
}
```

### `build_success`

Fired when a file compiles successfully.

```json
{
  "ts": "1711843200",
  "event": "build_success",
  "file": "src/validate.roca",
  "output": "out/validate.js"
}
```

### `build_failed`

Fired when a build fails (check errors or test failures).

```json
{
  "ts": "1711843200",
  "event": "build_failed",
  "file": "src/validate.roca",
  "reason": "proof tests failed"
}
```

## Using logs for bug tickets

The log contains the source code, error codes, and context for every failure. When filing a bug report, include the relevant log entries:

```bash
# Show recent failures
grep '"build_failed"\|"check_errors"\|"parse_error"' ~/.roca/logs/roca.jsonl | tail -5

# Show errors for a specific file
grep 'validate.roca' ~/.roca/logs/roca.jsonl | grep '"check_errors"' | tail -1 | jq .

# Show all test failures today
grep '"test_result"' ~/.roca/logs/roca.jsonl | grep '"failed":[1-9]' | tail -10
```

Include the JSON output in your [bug report](https://github.com/cameronrothenburg/roca/issues/new?template=bug_report.yml) — it gives maintainers the exact error codes, source context, and timeline.

## Disabling telemetry

Add `tracking = false` to your `roca.toml`:

```toml
[build]
tracking = false
```

When disabled, no events are written. The log file is not deleted — existing entries remain.

## Log rotation

The log file grows indefinitely. To rotate:

```bash
# Archive and clear
mv ~/.roca/logs/roca.jsonl ~/.roca/logs/roca-$(date +%Y%m%d).jsonl

# Or just truncate
> ~/.roca/logs/roca.jsonl
```
