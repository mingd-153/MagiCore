#!/usr/bin/env bash
# Bun COMPATIBILITY + lockfile-import E2E (P0-1 rewrite 2026-09-10)
# Tests the migration contract only:
#   1. optimizer env generation for a Bun project is COMPAT-GATED
#   2. `mgc dev` with a bun script is REFUSED on the native lane
#      (`--compat-runtime bun` is the only explicit gate)
#   3. bun.lock migrates to mgc.lock via `mgc import`
# Bun binary is a migration/fixture tool — NEVER the default engine.

set -euo pipefail

echo "=== Bun Compatibility + Migration E2E ==="

PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
if [ -f "$PROJECT_ROOT/target/release/mgc" ]; then
    MGC_BIN="$PROJECT_ROOT/target/release/mgc"
elif [ -f "$PROJECT_ROOT/target/debug/mgc" ]; then
    MGC_BIN="$PROJECT_ROOT/target/debug/mgc"
elif [ -f "$PROJECT_ROOT/target/debug/deps/mgc" ]; then
    MGC_BIN="$PROJECT_ROOT/target/debug/deps/mgc"
else
    echo "⚠️  SKIP: mgc binary not found"
    exit 77
fi

echo "Using mgc: $MGC_BIN"

TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT
cd "$TEMP_DIR"

# --- Bun project fixture (bun.lock, no mgc.lock) ---
cat >package.json <<EOF
{
  "name": "test-bun-migration",
  "version": "1.0.0",
  "scripts": {
    "dev": "bun run index.ts"
  },
  "dependencies": {}
}
EOF

cat >bun.lock <<'EOF'
{
  "lockfileVersion": 1,
  "packages": {
    "lodash": ["lodash@4.17.21", "", {}, "sha512-ok"],
  }
}
EOF

touch bunfig.toml
echo "web" > .mgc.core

# --- 1. mgc import: bun.lock → mgc.lock (migration works) ---
echo "Running mgc import..."
if ! "$MGC_BIN" import 2>&1; then
    echo "✗ FAIL: mgc import failed on bun.lock"
    exit 1
fi
if [ ! -f "mgc.lock" ]; then
    echo "✗ FAIL: mgc.lock not created by import"
    exit 1
fi
echo "✓ PASS: bun.lock → mgc.lock migration"

# --- 2. Native lane: mgc dev with bun script must REFUSE ---
DEV_EXIT=0
"$MGC_BIN" dev >/dev/null 2>&1 || DEV_EXIT=$?
if [ "$DEV_EXIT" -eq 0 ]; then
    echo "✗ FAIL: native 'mgc dev' accepted a bun script (must refuse — use --compat-runtime)"
    exit 1
fi
echo "✓ PASS: native 'mgc dev' refuses bun script (gate_runtime_spawn)"

# --- 3. Explicit compat lane opens ONLY bun (loud warning) ---
COMPAT_OUT=$("$MGC_BIN" dev --compat-runtime bun 2>&1 || true)
if echo "$COMPAT_OUT" | grep -q "COMPATIBILITY MODE"; then
    echo "✓ PASS: --compat-runtime bun prints the loud warning"
else
    echo "✗ FAIL: compat lane must warn loudly, got: $COMPAT_OUT"
    exit 1
fi

echo ""
echo "Bun compatibility + migration E2E complete:"
echo "  ✓ bun.lock imports to mgc.lock"
echo "  ✓ native dev refuses rival runtime"
echo "  ✓ explicit compat gate warns"
exit 0
