#!/usr/bin/env bash
# Distribution Smoke Test — basic verification without GitHub Release
# Status: BASIC IMPLEMENTATION (local binary testing)

set -euo pipefail

echo "=== Distribution Smoke Test (Basic) ==="
echo

# Find mgc binary (prefer local build)
PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
if [ -f "$PROJECT_ROOT/target/release/mgc" ]; then
    MGC_BIN="$PROJECT_ROOT/target/release/mgc"
    echo "Found local release binary: $MGC_BIN"
elif [ -f "$PROJECT_ROOT/target/debug/mgc" ]; then
    MGC_BIN="$PROJECT_ROOT/target/debug/mgc"
    echo "Found local debug binary: $MGC_BIN"
elif command -v mgc &>/dev/null; then
    MGC_BIN=$(command -v mgc)
    echo "Found mgc in PATH: $MGC_BIN"
else
    echo "✗ FAIL: mgc binary not found (not in PATH, not in target/)"
    exit 1
fi

echo
echo "--- Binary Verification ---"

# Check binary is executable
if [ -x "$MGC_BIN" ]; then
    echo "✓ Binary is executable"
else
    echo "✗ FAIL: Binary not executable"
    exit 1
fi

# Check version command works
if "$MGC_BIN" --version >/dev/null 2>&1; then
    VERSION=$("$MGC_BIN" --version)
    echo "✓ Version command works: $VERSION"
else
    echo "✗ FAIL: Version command failed"
    exit 1
fi

# Check help command works
if "$MGC_BIN" --help >/dev/null 2>&1; then
    echo "✓ Help command works"
else
    echo "✗ FAIL: Help command failed"
    exit 1
fi

# Check binary size
SIZE=$(stat -f%z "$MGC_BIN" 2>/dev/null || stat -c%s "$MGC_BIN" 2>/dev/null)
SIZE_MB=$((SIZE / 1024 / 1024))
echo "Binary size: ${SIZE_MB}MB"

if [ "$SIZE_MB" -lt 100 ]; then
    echo "✓ Binary size reasonable (<100MB)"
else
    echo "⚠️  Binary size large (${SIZE_MB}MB) - may impact distribution"
fi

echo
echo "--- Platform Detection ---"

OS=$(uname -s)
ARCH=$(uname -m)
echo "OS: $OS"
echo "Arch: $ARCH"

case "$OS" in
    Darwin)
        echo "✓ Platform: macOS"
        PLATFORM="macOS"
        ;;
    Linux)
        echo "✓ Platform: Linux"
        PLATFORM="Linux"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        echo "✓ Platform: Windows"
        PLATFORM="Windows"
        ;;
    *)
        echo "⚠️  Unknown platform: $OS"
        PLATFORM="Unknown"
        ;;
esac

echo
echo "--- Basic Smoke Test ---"

# Create temp test project
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

cd "$TEMP_DIR"

# Test create command
if "$MGC_BIN" create-web vanilla test-smoke >/dev/null 2>&1; then
    echo "✓ create-web command works"
    
    # Check project created
    if [ -d "test-smoke" ]; then
        echo "✓ Project directory created"
        
        # Check essential files
        if [ -f "test-smoke/mgc.toml" ] || [ -f "test-smoke/package.json" ]; then
            echo "✓ Project manifest exists (mgc.toml or package.json)"
        else
            echo "✗ FAIL: Project manifest missing"
            exit 1
        fi
    else
        echo "✗ FAIL: Project directory not created"
        exit 1
    fi
else
    echo "✗ FAIL: create-web command failed"
    exit 1
fi

echo
echo "--- Distribution Status ---"
echo
echo "✓ PASS: Basic smoke test complete on $PLATFORM ($ARCH)"
echo
echo "Limitations (no GitHub Release yet):"
echo "  • No Homebrew formula test"
echo "  • No Scoop manifest test"
echo "  • No SHA256 verification"
echo "  • No cross-platform matrix (tested current platform only)"
echo "  • No installation from published artifacts"
echo
echo "For full distribution readiness:"
echo "  1. Create GitHub Release v1.1.0-RC"
echo "  2. Build binaries for all platforms (7 targets)"
echo "  3. Upload to GitHub Releases with SHA256"
echo "  4. Update Homebrew/Scoop formulas"
echo "  5. Test installations on real machines"
echo
echo "Current test verifies:"
echo "  ✓ Binary executable on current platform"
echo "  ✓ Core commands work (--version, --help, create-web)"
echo "  ✓ Binary size reasonable"

exit 0
