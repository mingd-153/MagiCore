#!/usr/bin/env bash
# All-Core Full Lifecycle Test (Local)
# Verifies: create → install → test → build for all cores

set -euo pipefail

WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$WORKSPACE_ROOT"

echo "=== MagiCore All-Core Full Lifecycle Test ==="
echo ""

# Build
echo "Building mgc with all features..."
cargo build --release --bin mgc --no-default-features --features all --locked
MGC="$WORKSPACE_ROOT/target/release/mgc"

if [[ ! -x "$MGC" ]]; then
    echo "❌ FAIL: mgc binary not built"
    exit 1
fi

# Temp dir
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT
cd "$TEMP_DIR"

FAIL_COUNT=0

# Test Web
echo ""
echo "--- Web Lifecycle ---"
if "$MGC" create-web react test-web --yes && \
   test -f test-web/package.json && \
   cd test-web && \
   "$MGC" install && \
   test -d node_modules && \
   "$MGC" build && \
   { test -d dist || test -d build; } && \
   cd ..; then
    echo "✅ Web: PASS"
else
    echo "❌ Web: FAIL"
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# Test AI
echo ""
echo "--- AI Lifecycle ---"
if command -v python3 >/dev/null 2>&1 && \
   "$MGC" create-ai python-agent test-ai && \
   test -f test-ai/pyproject.toml && \
   cd test-ai && \
   python3 -m venv .venv && \
   . .venv/bin/activate && \
   python -m pip install build pytest && \
   "$MGC" install && \
   mkdir -p tests && \
   printf 'from agent import AIAgent\n\ndef test_agent():\n    assert AIAgent().run("local")\n' > tests/test_agent.py && \
   PYTHONPATH=src "$MGC" test && \
   "$MGC" build && \
   test -d dist && \
   cd ..; then
    echo "✅ AI: PASS"
else
    echo "❌ AI: FAIL"
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# Test App
echo ""
echo "--- App Lifecycle ---"
if command -v flutter >/dev/null 2>&1 && \
   "$MGC" create-app flutter test_app && \
   test -f test_app/pubspec.yaml && \
   cd test_app && \
   "$MGC" install && \
   test -f pubspec.lock && \
   "$MGC" test && \
   "$MGC" build && \
   test -d build/flutter_assets && \
   cd ..; then
    echo "✅ App: PASS"
else
    echo "❌ App: FAIL (Flutter is required for all-core verification)"
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# Test Lib (Rust)
echo ""
echo "--- Lib Lifecycle (Rust) ---"
if "$MGC" create-lib rust test-lib && \
   test -f test-lib/Cargo.toml && \
   cd test-lib && \
   "$MGC" install && \
   "$MGC" build && \
   "$MGC" test && \
   cd ..; then
    echo "✅ Lib: PASS"
else
    echo "❌ Lib: FAIL"
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# Summary
echo ""
echo "=== SUMMARY ==="
echo "FAIL: $FAIL_COUNT"
echo ""

if [[ $FAIL_COUNT -eq 0 ]]; then
    echo "✅ ALL CORES LIFECYCLE VERIFIED"
    exit 0
else
    echo "❌ FAIL: $FAIL_COUNT cores failed"
    exit 1
fi
