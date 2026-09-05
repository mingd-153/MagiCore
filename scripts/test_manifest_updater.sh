#!/usr/bin/env bash
set -euo pipefail

# Integration test for manifest updater
# Creates fake artifacts and verifies manifest updates

echo "=== Manifest Updater Integration Test ==="

# Create temp directories
TEST_DIR=$(mktemp -d)
ARTIFACTS_DIR="$TEST_DIR/artifacts"
MANIFEST_DIR="$TEST_DIR/manifests"
mkdir -p "$ARTIFACTS_DIR" "$MANIFEST_DIR/homebrew" "$MANIFEST_DIR/scoop"

trap 'rm -rf "$TEST_DIR"' EXIT

VERSION="1.1.0-rc.3"

echo "Creating fake artifacts for version $VERSION..."

# Create fake artifacts with known content
echo "fake magicore linux" > "$ARTIFACTS_DIR/magicore-${VERSION}-linux-x64.tar.gz"
echo "fake magicore macos" > "$ARTIFACTS_DIR/magicore-${VERSION}-macos-x64.tar.gz"
echo "fake magicore windows" > "$ARTIFACTS_DIR/magicore-${VERSION}-windows-x64.zip"
echo "fake magicore-web linux" > "$ARTIFACTS_DIR/magicore-web-${VERSION}-linux-x64.tar.gz"
echo "fake magicore-web macos" > "$ARTIFACTS_DIR/magicore-web-${VERSION}-macos-x64.tar.gz"
echo "fake magicore-web windows" > "$ARTIFACTS_DIR/magicore-web-${VERSION}-windows-x64.zip"

echo "✓ Created 6 fake artifacts"

# Compute expected hashes
if command -v shasum >/dev/null 2>&1; then
  HASH_CMD="shasum -a 256"
elif command -v sha256sum >/dev/null 2>&1; then
  HASH_CMD="sha256sum"
else
  echo "❌ No SHA256 tool found"
  exit 1
fi

HASH_LINUX=$($HASH_CMD "$ARTIFACTS_DIR/magicore-${VERSION}-linux-x64.tar.gz" | awk '{print $1}')
HASH_MACOS=$($HASH_CMD "$ARTIFACTS_DIR/magicore-${VERSION}-macos-x64.tar.gz" | awk '{print $1}')
HASH_WINDOWS=$($HASH_CMD "$ARTIFACTS_DIR/magicore-${VERSION}-windows-x64.zip" | awk '{print $1}')

echo "✓ Computed hashes"

# Run manifest updater
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export MAGICORE_REPO_ROOT="$MANIFEST_DIR"

echo "Running manifest updater..."
"$SCRIPT_DIR/update-manifests.sh" --version "$VERSION" --artifacts "$ARTIFACTS_DIR"

echo ""
echo "=== Verification ==="

# Test 1: Check Homebrew formula has correct version
if ! grep -q "version \"$VERSION\"" "$MANIFEST_DIR/packaging/homebrew/magicore.rb"; then
  echo "❌ Test 1 FAIL: Version not updated in magicore.rb"
  exit 1
fi
echo "✓ Test 1 PASS: Version correct in magicore.rb"

# Test 2: Check Homebrew formula has correct macOS hash
if ! grep -q "$HASH_MACOS" "$MANIFEST_DIR/packaging/homebrew/magicore.rb"; then
  echo "❌ Test 2 FAIL: macOS hash not found in magicore.rb"
  exit 1
fi
echo "✓ Test 2 PASS: macOS hash correct"

# Test 3: Check Homebrew formula has correct Linux hash
if ! grep -q "$HASH_LINUX" "$MANIFEST_DIR/packaging/homebrew/magicore.rb"; then
  echo "❌ Test 3 FAIL: Linux hash not found in magicore.rb"
  exit 1
fi
echo "✓ Test 3 PASS: Linux hash correct"

# Test 4: Check Homebrew formula uses new artifact naming
if ! grep -q "magicore-${VERSION}-macos-x64.tar.gz" "$MANIFEST_DIR/packaging/homebrew/magicore.rb"; then
  echo "❌ Test 4 FAIL: New artifact naming not used in magicore.rb"
  exit 1
fi
echo "✓ Test 4 PASS: New artifact naming in magicore.rb"

# Test 5: Check Homebrew formula has ARM64 error message
if ! grep -q "odie \"ARM64 not yet supported" "$MANIFEST_DIR/packaging/homebrew/magicore.rb"; then
  echo "❌ Test 5 FAIL: ARM64 error message not found"
  exit 1
fi
echo "✓ Test 5 PASS: ARM64 error message present"

# Test 6: Check Scoop manifest has correct version
if ! grep -q "\"version\": \"$VERSION\"" "$MANIFEST_DIR/packaging/scoop/magicore.json"; then
  echo "❌ Test 6 FAIL: Version not updated in magicore.json"
  exit 1
fi
echo "✓ Test 6 PASS: Version correct in magicore.json"

# Test 7: Check Scoop manifest has correct hash
if ! grep -q "$HASH_WINDOWS" "$MANIFEST_DIR/packaging/scoop/magicore.json"; then
  echo "❌ Test 7 FAIL: Windows hash not found in magicore.json"
  exit 1
fi
echo "✓ Test 7 PASS: Windows hash correct"

# Test 8: Check Scoop manifest uses new artifact naming
if ! grep -q "magicore-${VERSION}-windows-x64.zip" "$MANIFEST_DIR/packaging/scoop/magicore.json"; then
  echo "❌ Test 8 FAIL: New artifact naming not used in magicore.json"
  exit 1
fi
echo "✓ Test 8 PASS: New artifact naming in magicore.json"

# Test 9: Check no PLACEHOLDER remains
if grep -r "PLACEHOLDER_WILL_BE_REPLACED_BY_CI\|UPDATE_ME" "$MANIFEST_DIR/packaging/" >/dev/null 2>&1; then
  echo "❌ Test 9 FAIL: Found PLACEHOLDER or UPDATE_ME"
  exit 1
fi
echo "✓ Test 9 PASS: No placeholders found"

# Test 10: Check no old naming pattern
if grep -r "magicore-Linux-X64\|magicore-macOS-X64" "$MANIFEST_DIR/packaging/" >/dev/null 2>&1; then
  echo "❌ Test 10 FAIL: Found old naming pattern"
  exit 1
fi
echo "✓ Test 10 PASS: No old naming patterns"

# Test 11: Run verify mode
echo ""
echo "Testing verify mode..."
if ! "$SCRIPT_DIR/update-manifests.sh" --version "$VERSION" --artifacts "$ARTIFACTS_DIR" --verify-only; then
  echo "❌ Test 11 FAIL: Verify mode failed"
  exit 1
fi
echo "✓ Test 11 PASS: Verify mode succeeded"

# Test 12: Verify fails with wrong hash
echo ""
echo "Testing verify detects incorrect hash..."
sed -i.bak "s/$HASH_MACOS/0000000000000000000000000000000000000000000000000000000000000000/" "$MANIFEST_DIR/packaging/homebrew/magicore.rb"
if "$SCRIPT_DIR/update-manifests.sh" --version "$VERSION" --artifacts "$ARTIFACTS_DIR" --verify-only 2>/dev/null; then
  echo "❌ Test 12 FAIL: Verify should have failed with wrong hash"
  exit 1
fi
echo "✓ Test 12 PASS: Verify correctly detects wrong hash"

echo ""
echo "========================================"
echo "✅ All 12 integration tests PASSED"
echo "========================================"
