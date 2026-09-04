#!/usr/bin/env bash
# Smoke test for MagiCore release artifacts
# Tests: version check, binary location, basic functionality
# Usage: ./smoke-test.sh [--mgc-path /path/to/mgc]

set -euo pipefail

MGC_PATH="${MGC_PATH:-mgc}"
FAIL_COUNT=0

# Parse args
while [[ $# -gt 0 ]]; do
  case "$1" in
    --mgc-path)
      MGC_PATH="$2"
      shift 2
      ;;
    -h|--help)
      echo "Usage: $0 [--mgc-path /path/to/mgc]"
      echo ""
      echo "Smoke test for MagiCore binary. Tests:"
      echo "  - Version check (mgc --version)"
      echo "  - Binary location (which/where mgc)"
      echo "  - Help output (mgc --help)"
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

echo "=== MagiCore Smoke Test ==="
echo "Testing binary: $MGC_PATH"
echo ""

# Test 1: mgc --version
echo "Test 1: mgc --version"
if ! version_output=$("$MGC_PATH" --version 2>&1); then
  echo "❌ FAIL: mgc --version failed"
  ((FAIL_COUNT++))
else
  echo "✅ PASS: $version_output"
fi
echo ""

# Test 2: Binary location
echo "Test 2: Binary location"
LOCATION_FOUND=0
if command -v which >/dev/null 2>&1; then
  if which_output=$(which "$MGC_PATH" 2>&1); then
    echo "✅ PASS: Binary found at: $which_output"
    LOCATION_FOUND=1
  else
    echo "❌ FAIL: which mgc failed - binary not in PATH"
    ((FAIL_COUNT++))
  fi
elif command -v where >/dev/null 2>&1; then
  # Windows
  if where_output=$(where "$MGC_PATH" 2>&1); then
    echo "✅ PASS: Binary found at: $where_output"
    LOCATION_FOUND=1
  else
    echo "❌ FAIL: where mgc failed - binary not in PATH"
    ((FAIL_COUNT++))
  fi
else
  echo "❌ FAIL: No which/where command available - cannot verify binary location"
  ((FAIL_COUNT++))
fi
echo ""

# Test 3: mgc --help
echo "Test 3: mgc --help"
if ! help_output=$("$MGC_PATH" --help 2>&1); then
  echo "❌ FAIL: mgc --help failed"
  ((FAIL_COUNT++))
else
  # Check help contains expected text
  if echo "$help_output" | grep -q "MagiCore"; then
    echo "✅ PASS: Help output looks good"
  else
    echo "⚠️  WARN: Help output doesn't contain expected text"
    echo "Output: $help_output"
  fi
fi
echo ""

# Test 4: mgc version (subcommand)
echo "Test 4: mgc version (subcommand)"
if version_cmd=$("$MGC_PATH" version 2>&1); then
  echo "✅ PASS: $version_cmd"
else
  echo "❌ FAIL: mgc version subcommand not working"
  echo "This command should be implemented for consistency"
  ((FAIL_COUNT++))
fi
echo ""

# Summary
echo "=== Smoke Test Summary ==="
if [[ $FAIL_COUNT -eq 0 ]]; then
  echo "✅ All critical tests PASSED"
  exit 0
else
  echo "❌ $FAIL_COUNT critical test(s) FAILED"
  exit 1
fi
