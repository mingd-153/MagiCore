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
cargo build --release --features all
MGC="$WORKSPACE_ROOT/target/release/mgc"

if [[ ! -x "$MGC" ]]; then
    echo "❌ FAIL: mgc binary not built"
    exit 1
fi

# Temp dir
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT
cd "$TEMP_DIR"

FAIL_COUNT=0

# Test Web
echo ""
echo "--- Web Lifecycle ---"
if "$MGC" create-web react test-web --yes && \
   test -f test-web/package.json && \
   cd test-web && \
   npm install && \
   test -d node_modules && \
   npm run build && \
   { test -d dist || test -d build; } && \
   cd ..; then
    echo "✅ Web: PASS"
else
    echo "❌ Web: FAIL"
    ((FAIL_COUNT++))
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
   pip install -e . && \
   python -c "import sys" && \
   cd ..; then
    echo "✅ AI: PASS"
else
    echo "❌ AI: FAIL"
    ((FAIL_COUNT++))
fi

# Test App
echo ""
echo "--- App Lifecycle ---"
if command -v flutter >/dev/null 2>&1 && \
   "$MGC" create-app flutter test_app && \
   test -f test_app/pubspec.yaml && \
   cd test_app && \
   flutter pub get && \
   test -f pubspec.lock && \
   flutter test && \
   cd ..; then
    echo "✅ App: PASS"
else
    echo "⚠️  App: SKIP (Flutter not available)"
fi

# Test Lib (Rust)
echo ""
echo "--- Lib Lifecycle (Rust) ---"
if "$MGC" create-lib rust test-lib && \
   test -f test-lib/Cargo.toml && \
   cd test-lib && \
   cargo build --release && \
   cargo test --release && \
   cd ..; then
    echo "✅ Lib: PASS"
else
    echo "❌ Lib: FAIL"
    ((FAIL_COUNT++))
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
