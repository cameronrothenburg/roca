#!/bin/bash
# Roca JS output test runner
# Tests compiled JS against the local @rocalang/runtime package
# Usage: ./test.sh [file.roca]  — test one file, or all in cases/

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RUNTIME_DIR="$SCRIPT_DIR/../../packages/runtime"
OUT_DIR="$SCRIPT_DIR/out"

# Setup
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

# Write package.json linking local runtime
cat > "$OUT_DIR/package.json" << EOF
{
  "name": "roca-js-tests",
  "private": true,
  "type": "module",
  "dependencies": {
    "@rocalang/runtime": "file:$RUNTIME_DIR"
  }
}
EOF

# Install local runtime
cd "$OUT_DIR"
npm install --silent 2>/dev/null
cd "$SCRIPT_DIR"

# Build compiler
ROCA="cargo run --quiet --manifest-path $SCRIPT_DIR/../../Cargo.toml --"

passed=0
failed=0
errors=""

run_test() {
    local roca_file="$1"
    local name=$(basename "$roca_file" .roca)

    # Compile .roca to JS
    if ! $ROCA build "$roca_file" 2>/dev/null; then
        echo "  FAIL: $name (compile error)"
        failed=$((failed + 1))
        errors="$errors\n  $name: compile error"
        return
    fi

    # Find the compiled JS
    local js_file="$OUT_DIR/$name.js"
    if [ ! -f "$js_file" ]; then
        # Check in out/ subdirectory
        local src_out=$(dirname "$roca_file")/out
        if [ -f "$src_out/$name.js" ]; then
            cp "$src_out/$name.js" "$js_file"
        else
            echo "  FAIL: $name (no JS output)"
            failed=$((failed + 1))
            errors="$errors\n  $name: no JS output found"
            return
        fi
    fi

    # Run via Node
    if node "$js_file" 2>/dev/null; then
        echo "  OK: $name"
        passed=$((passed + 1))
    else
        echo "  FAIL: $name (runtime error)"
        failed=$((failed + 1))
        errors="$errors\n  $name: runtime error (exit $?)"
    fi
}

echo "=== Roca JS Output Tests ==="
echo ""

if [ -n "$1" ]; then
    # Single file
    run_test "$1"
else
    # All .roca files in cases/
    if [ -d "$SCRIPT_DIR/cases" ]; then
        for f in "$SCRIPT_DIR/cases"/*.roca; do
            [ -f "$f" ] && run_test "$f"
        done
    else
        echo "No cases/ directory. Create tests/js/cases/*.roca files."
        exit 1
    fi
fi

echo ""
echo "=== $passed passed, $failed failed ==="

if [ $failed -gt 0 ]; then
    echo -e "\nFailures:$errors"
    exit 1
fi
