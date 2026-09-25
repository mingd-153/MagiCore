#!/usr/bin/env bash
# Deno optimizer environment generation test
# Status: CONFIG GENERATION (does not execute the Deno child process)

set -euo pipefail

echo "=== Deno Optimizer Environment Generation Test ==="

# Find mgc binary (prefer local build over system install)
PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
if [ -f "$PROJECT_ROOT/target/release/mgc" ]; then
    MGC_BIN="$PROJECT_ROOT/target/release/mgc"
elif [ -f "$PROJECT_ROOT/target/debug/mgc" ]; then
    MGC_BIN="$PROJECT_ROOT/target/debug/mgc"
elif command -v mgc &>/dev/null; then
    MGC_BIN="mgc"
else
    echo "⚠️  SKIP: mgc binary not found"
    exit 77
fi

echo "Using mgc: $MGC_BIN"

# Create temp project
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

cd "$TEMP_DIR"

# Create minimal deno project
cat >deno.json <<EOF
{
  "tasks": {
    "dev": "deno run main.ts"
  }
}
EOF

cat >main.ts <<EOF
console.log("Deno runtime test");
console.log("DENO_V8_FLAGS:", Deno.env.get("DENO_V8_FLAGS") || "not set");
EOF

# Mark as web core for optimizer
echo "web" > .mgc.core

# Run mgc optimizer
echo "Running mgc optimizer..."
if ! "$MGC_BIN" optimizer 2>&1; then
    echo "✗ FAIL: mgc optimizer failed"
    exit 1
fi

# Check if deno_env.env was created
if [ ! -f ".mgc-optimizer/deno_env.env" ]; then
    echo "✗ FAIL: deno_env.env not created"
    exit 1
fi

echo "✓ PASS: deno_env.env created"

# Verify content
if grep -q "DENO_V8_FLAGS" ".mgc-optimizer/deno_env.env"; then
    echo "✓ PASS: DENO_V8_FLAGS in env file"
else
    echo "✗ FAIL: DENO_V8_FLAGS not in env file"
    exit 1
fi

echo "✓ PASS: Deno optimizer environment generation complete"
echo ""
echo "Note: Full E2E (mgc dev with env consumer) requires integration test"
echo "      Current test verifies: optimizer detection + env generation"

exit 0
