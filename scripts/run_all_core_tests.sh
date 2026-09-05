#!/usr/bin/env bash
# All-Core Scaffold Verification (Local)
# Verifies CLI commands exist and can create projects
# Scope: create → verify files exist (NOT install/test/build)

set -euo pipefail

WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$WORKSPACE_ROOT"

echo "=== MagiCore All-Core Scaffold Test ==="
echo "Scope: CLI + scaffold templates only"
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

# Track results
FAIL_COUNT=0
PASS_COUNT=0

# Test Web
echo ""
echo "--- Testing Web Scaffold ---"
if "$MGC" create-web react test-web --yes && \
   test -d test-web && \
   test -f test-web/package.json; then
    echo "✅ Web: PASS"
    ((PASS_COUNT++))
else
    echo "❌ Web: FAIL"
    ((FAIL_COUNT++))
fi

# Test AI
echo ""
echo "--- Testing AI Scaffold ---"
if "$MGC" create-ai python-agent test-ai && \
   test -d test-ai && \
   { test -f test-ai/pyproject.toml || test -f test-ai/requirements.txt; }; then
    echo "✅ AI: PASS"
    ((PASS_COUNT++))
else
    echo "❌ AI: FAIL"
    ((FAIL_COUNT++))
fi

# Test App
echo ""
echo "--- Testing App Scaffold ---"
if "$MGC" create-app flutter test_app && \
   test -d test_app && \
   test -f test_app/pubspec.yaml; then
    echo "✅ App: PASS"
    ((PASS_COUNT++))
else
    echo "❌ App: FAIL"
    ((FAIL_COUNT++))
fi

# Test Lib (Rust only for local speed)
echo ""
echo "--- Testing Lib Scaffold (Rust) ---"
if "$MGC" create-lib rust test-lib && \
   test -d test-lib && \
   test -f test-lib/Cargo.toml; then
    echo "✅ Lib: PASS"
    ((PASS_COUNT++))
else
    echo "❌ Lib: FAIL"
    ((FAIL_COUNT++))
fi

# Summary
echo ""
echo "=== SUMMARY ==="
echo "PASS: $PASS_COUNT"
echo "FAIL: $FAIL_COUNT"
echo ""
echo "Scope: Scaffold only (CLI + templates)"
echo "NOT tested: install/test/build/run/optimizer/cache"
echo ""

# Gate: ALL must pass
if [[ $FAIL_COUNT -eq 0 ]]; then
    echo "✅ ALL CORES SCAFFOLD VERIFIED"
    exit 0
else
    echo "❌ FAIL: $FAIL_COUNT cores failed scaffold"
    exit 1
fi
