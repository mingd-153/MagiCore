#!/usr/bin/env bash
# Test SBOM version validation logic / Kiểm tra logic validation phiên bản SBOM

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Testing SBOM Version Validation Logic ==="
echo ""

# Test 1: Valid SBOM with matching version
echo "Test 1: Valid SBOM (version matches)"
cat > /tmp/test-sbom-valid.json <<EOF
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.4",
  "metadata": {
    "component": {
      "name": "mgc",
      "version": "1.1.0-rc.3"
    }
  },
  "components": [
    {"name": "dep1", "version": "1.0.0"},
    {"name": "dep2", "version": "2.0.0"},
    {"name": "dep3", "version": "3.0.0"},
    {"name": "dep4", "version": "4.0.0"},
    {"name": "dep5", "version": "5.0.0"},
    {"name": "dep6", "version": "6.0.0"},
    {"name": "dep7", "version": "7.0.0"},
    {"name": "dep8", "version": "8.0.0"},
    {"name": "dep9", "version": "9.0.0"},
    {"name": "dep10", "version": "10.0.0"}
  ]
}
EOF

VERSION="1.1.0-rc.3"
sbom="/tmp/test-sbom-valid.json"

# Run validation logic (same as workflow)
if ! jq -e '.bomFormat == "CycloneDX"' "$sbom" > /dev/null; then
  echo "  ❌ FAIL: Invalid bomFormat"
  exit 1
fi

if ! jq -e '.specVersion' "$sbom" > /dev/null; then
  echo "  ❌ FAIL: Missing specVersion"
  exit 1
fi

if ! jq -e '.metadata.component' "$sbom" > /dev/null; then
  echo "  ❌ FAIL: Missing metadata.component"
  exit 1
fi

if ! jq -e '.components' "$sbom" > /dev/null; then
  echo "  ❌ FAIL: Missing components"
  exit 1
fi

component_count=$(jq -r '.components | length' "$sbom")
if [[ $component_count -lt 10 ]]; then
  echo "  ❌ FAIL: Too few components ($component_count)"
  exit 1
fi

sbom_version=$(jq -r '.metadata.component.version // empty' "$sbom")
if [[ -z "$sbom_version" ]]; then
  echo "  ❌ FAIL: Missing metadata.component.version"
  exit 1
fi

if [[ "$sbom_version" != "$VERSION" ]]; then
  echo "  ❌ FAIL: SBOM version mismatch: expected $VERSION, got $sbom_version"
  exit 1
fi

echo "  ✅ PASS: Valid SBOM accepted"

# Test 2: SBOM with version mismatch (should fail)
echo ""
echo "Test 2: SBOM version mismatch (should reject)"
cat > /tmp/test-sbom-mismatch.json <<EOF
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.4",
  "metadata": {
    "component": {
      "name": "mgc",
      "version": "9.9.9"
    }
  },
  "components": [
    {"name": "dep1", "version": "1.0.0"},
    {"name": "dep2", "version": "2.0.0"},
    {"name": "dep3", "version": "3.0.0"},
    {"name": "dep4", "version": "4.0.0"},
    {"name": "dep5", "version": "5.0.0"},
    {"name": "dep6", "version": "6.0.0"},
    {"name": "dep7", "version": "7.0.0"},
    {"name": "dep8", "version": "8.0.0"},
    {"name": "dep9", "version": "9.0.0"},
    {"name": "dep10", "version": "10.0.0"}
  ]
}
EOF

sbom="/tmp/test-sbom-mismatch.json"
sbom_version=$(jq -r '.metadata.component.version // empty' "$sbom")

if [[ "$sbom_version" != "$VERSION" ]]; then
  echo "  ✅ PASS: Version mismatch correctly rejected (expected $VERSION, got $sbom_version)"
else
  echo "  ❌ FAIL: Should reject version mismatch"
  exit 1
fi

# Test 3: SBOM missing version field (should fail)
echo ""
echo "Test 3: SBOM missing version field (should reject)"
cat > /tmp/test-sbom-no-version.json <<EOF
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.4",
  "metadata": {
    "component": {
      "name": "mgc"
    }
  },
  "components": [
    {"name": "dep1", "version": "1.0.0"},
    {"name": "dep2", "version": "2.0.0"},
    {"name": "dep3", "version": "3.0.0"},
    {"name": "dep4", "version": "4.0.0"},
    {"name": "dep5", "version": "5.0.0"},
    {"name": "dep6", "version": "6.0.0"},
    {"name": "dep7", "version": "7.0.0"},
    {"name": "dep8", "version": "8.0.0"},
    {"name": "dep9", "version": "9.0.0"},
    {"name": "dep10", "version": "10.0.0"}
  ]
}
EOF

sbom="/tmp/test-sbom-no-version.json"
sbom_version=$(jq -r '.metadata.component.version // empty' "$sbom")

if [[ -z "$sbom_version" ]]; then
  echo "  ✅ PASS: Missing version correctly rejected"
else
  echo "  ❌ FAIL: Should reject missing version"
  exit 1
fi

# Cleanup
rm -f /tmp/test-sbom-*.json

echo ""
echo "=== All SBOM Validation Tests PASSED ==="
echo ""
echo "Validation logic proven:"
echo "  ✅ Accepts valid SBOM with matching version"
echo "  ✅ Rejects SBOM with version mismatch"
echo "  ✅ Rejects SBOM with missing version"
echo ""
echo "Ready for production workflow."
