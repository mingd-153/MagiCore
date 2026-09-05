#!/usr/bin/env bash
# Test artifact naming contract / Kiểm tra quy ước đặt tên artifact

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACT="${SCRIPT_DIR}/release-artifact-contract.sh"

FAILED=0
PASSED=0

test_case() {
    local name="$1"
    local version="$2"
    local os="$3"
    local arch="$4"
    local variant="$5"
    local expected_archive="$6"
    local expected_checksum="$7"
    local expected_sbom="$8"

    echo "Testing: $name"

    output=$("$CONTRACT" "$version" "$os" "$arch" "$variant" 2>&1)

    # Extract values / Lấy giá trị
    archive=$(echo "$output" | grep "^ARCHIVE=" | cut -d= -f2)
    checksum=$(echo "$output" | grep "^CHECKSUM=" | cut -d= -f2)
    sbom=$(echo "$output" | grep "^SBOM=" | cut -d= -f2)

    if [[ "$archive" != "$expected_archive" ]]; then
        echo "  ❌ FAIL: Archive name mismatch"
        echo "     Expected: $expected_archive"
        echo "     Got:      $archive"
        FAILED=$((FAILED + 1))
        return 1
    fi

    if [[ "$checksum" != "$expected_checksum" ]]; then
        echo "  ❌ FAIL: Checksum name mismatch"
        echo "     Expected: $expected_checksum"
        echo "     Got:      $checksum"
        FAILED=$((FAILED + 1))
        return 1
    fi

    if [[ "$sbom" != "$expected_sbom" ]]; then
        echo "  ❌ FAIL: SBOM name mismatch"
        echo "     Expected: $expected_sbom"
        echo "     Got:      $sbom"
        FAILED=$((FAILED + 1))
        return 1
    fi

    echo "  ✅ PASS"
    PASSED=$((PASSED + 1))
    return 0
}

echo "=== Testing Artifact Naming Contract ==="
echo ""

# Test all-core variants
test_case "Linux all-core" \
    "1.1.0" "linux" "x64" "all" \
    "magicore-1.1.0-linux-x64.tar.gz" \
    "magicore-1.1.0-linux-x64.tar.gz.sha256" \
    "magicore-1.1.0-linux-x64-sbom.json"

test_case "macOS all-core" \
    "1.1.0" "macos" "x64" "all" \
    "magicore-1.1.0-macos-x64.tar.gz" \
    "magicore-1.1.0-macos-x64.tar.gz.sha256" \
    "magicore-1.1.0-macos-x64-sbom.json"

test_case "Windows all-core" \
    "1.1.0" "windows" "x64" "all" \
    "magicore-1.1.0-windows-x64.zip" \
    "magicore-1.1.0-windows-x64.zip.sha256" \
    "magicore-1.1.0-windows-x64-sbom.json"

# Test web-only variants
test_case "Linux web-only" \
    "1.1.0" "linux" "x64" "web" \
    "magicore-web-1.1.0-linux-x64.tar.gz" \
    "magicore-web-1.1.0-linux-x64.tar.gz.sha256" \
    "magicore-web-1.1.0-linux-x64-sbom.json"

test_case "macOS web-only" \
    "1.1.0" "macos" "x64" "web" \
    "magicore-web-1.1.0-macos-x64.tar.gz" \
    "magicore-web-1.1.0-macos-x64.tar.gz.sha256" \
    "magicore-web-1.1.0-macos-x64-sbom.json"

test_case "Windows web-only" \
    "1.1.0" "windows" "x64" "web" \
    "magicore-web-1.1.0-windows-x64.zip" \
    "magicore-web-1.1.0-windows-x64.zip.sha256" \
    "magicore-web-1.1.0-windows-x64-sbom.json"

# Test RC version
test_case "RC version" \
    "1.1.0-rc.3" "linux" "x64" "web" \
    "magicore-web-1.1.0-rc.3-linux-x64.tar.gz" \
    "magicore-web-1.1.0-rc.3-linux-x64.tar.gz.sha256" \
    "magicore-web-1.1.0-rc.3-linux-x64-sbom.json"

# Test invalid inputs
echo "Testing: Invalid OS"
if "$CONTRACT" "1.1.0" "invalid" "x64" "all" 2>/dev/null; then
    echo "  ❌ FAIL: Should reject invalid OS"
    FAILED=$((FAILED + 1))
else
    echo "  ✅ PASS: Rejected invalid OS"
    PASSED=$((PASSED + 1))
fi

echo "Testing: Invalid variant"
if "$CONTRACT" "1.1.0" "linux" "x64" "invalid" 2>/dev/null; then
    echo "  ❌ FAIL: Should reject invalid variant"
    FAILED=$((FAILED + 1))
else
    echo "  ✅ PASS: Rejected invalid variant"
    PASSED=$((PASSED + 1))
fi

echo ""
echo "========================================"
echo "Contract Tests: ${PASSED} passed, ${FAILED} failed"
echo "========================================"

if [[ $FAILED -gt 0 ]]; then
    exit 1
fi

echo "✅ All contract tests PASSED"
exit 0
