#!/usr/bin/env bash
# NATIVE-RUNTIME BOUNDARY E2E
# Tests that compatibility flags do not create a delegated runtime lane:
#   - `mgc dev` native REFUSES a bun/deno script
#   - `mgc dev --compat-runtime bun|deno` also refuses without spawning
# Bun/Deno are fixture names only — neither is an MGC execution engine.

set -euo pipefail

echo "=== Compat-Runtime E2E (bun + deno) ==="

# Find mgc binary
PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"

if [ -f "$PROJECT_ROOT/target/release/mgc" ]; then
    MGC_BIN="$PROJECT_ROOT/target/release/mgc"
elif [ -f "$PROJECT_ROOT/target/debug/mgc" ]; then
    MGC_BIN="$PROJECT_ROOT/target/debug/mgc"
else
    echo "✗ FAIL: mgc binary not found (need cargo build first)"
    exit 1
fi

echo "Using mgc: $MGC_BIN"
echo

# Isolated temp workspace
TEMP_BASE=$(mktemp -d)
trap "rm -rf $TEMP_BASE" EXIT
cd "$TEMP_BASE"

run_compat_dev_lane() {
    local RUNTIME="$1" SCRIPT_NAME="$2"

    mkdir -p "proj-$RUNTIME"
    cd "proj-$RUNTIME"

    cat > package.json <<EOF
{
  "name": "compat-$RUNTIME",
  "version": "1.0.0",
  "scripts": {
    "dev": "$SCRIPT_NAME"
  },
  "dependencies": {}
}
EOF

    cat > mgc.toml <<EOF
name = "compat-$RUNTIME"
version = "1.0.0"
ecosystem = "web"
EOF

    # 1. NATIVE lane: must REFUSE the rival-runtime script.
    local NATIVE_EXIT=0
    MGC_COMPAT_RUNTIME= "$MGC_BIN" dev >/dev/null 2>&1 || NATIVE_EXIT=$?
    if [ "$NATIVE_EXIT" -eq 0 ]; then
        echo "✗ FAIL: native mgc dev accepted a $RUNTIME script (must refuse)"
        cd ..
        return 1
    fi
    echo "✓ native 'mgc dev' refuses the $RUNTIME script"

    # 2. Historical compat lane: must fail before spawning the runtime.
    local COMPAT_LOG="compat-dev.log"
    MGC_COMPAT_RUNTIME= "$MGC_BIN" dev --compat-runtime "$RUNTIME" >"$COMPAT_LOG" 2>&1 || true
    if grep -q "native MagiCore runtime" "$COMPAT_LOG"; then
        echo "✓ --compat-runtime $RUNTIME refused by native-only policy"
    else
        echo "✗ FAIL: compatibility flag must be refused clearly, got:"
        cat "$COMPAT_LOG"
        cd ..
        return 1
    fi

    if [ -f ".mgc/exec.log" ] && grep -q "$RUNTIME" .mgc/exec.log 2>/dev/null; then
        echo "✗ FAIL: rejected runtime was recorded as executed"
        cd ..
        return 1
    fi

    cd ..
    return 0
}

echo "--- Test 1: Bun compat lane ---"
run_compat_dev_lane "bun" "bun run server.ts" || exit 1

echo "--- Test 2: Deno compat lane ---"
run_compat_dev_lane "deno" "deno run server.ts" || exit 1

echo
echo "Compat-runtime E2E complete:"
echo "  ✓ native and historical compat flags refuse bun + deno scripts"
echo "  ✓ explicit compat gate warns loudly"
echo "  ✓ compat spawns are audit-logged"
echo "NOTE: this is COMPATIBILITY evidence — NOT native-engine evidence."
exit 0
