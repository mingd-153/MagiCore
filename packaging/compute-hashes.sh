#!/usr/bin/env bash
# Compute SHA256 hashes for release artifacts
# Usage: ./compute-hashes.sh <release-dir>

set -euo pipefail

RELEASE_DIR="${1:-.}"

echo "Computing SHA256 hashes for artifacts in: $RELEASE_DIR"
echo ""

# macOS
if [ -f "$RELEASE_DIR/magicore-macOS-ARM64.tar.gz" ]; then
    echo "macOS ARM64:"
    shasum -a 256 "$RELEASE_DIR/magicore-macOS-ARM64.tar.gz"
    echo ""
fi

if [ -f "$RELEASE_DIR/magicore-macOS-X64.tar.gz" ]; then
    echo "macOS X64:"
    shasum -a 256 "$RELEASE_DIR/magicore-macOS-X64.tar.gz"
    echo ""
fi

# Linux
if [ -f "$RELEASE_DIR/magicore-Linux-ARM64.tar.gz" ]; then
    echo "Linux ARM64:"
    shasum -a 256 "$RELEASE_DIR/magicore-Linux-ARM64.tar.gz"
    echo ""
fi

if [ -f "$RELEASE_DIR/magicore-Linux-X64.tar.gz" ]; then
    echo "Linux X64:"
    shasum -a 256 "$RELEASE_DIR/magicore-Linux-X64.tar.gz"
    echo ""
fi

# Windows
if [ -f "$RELEASE_DIR/magicore-Windows-X64.zip" ]; then
    echo "Windows X64:"
    shasum -a 256 "$RELEASE_DIR/magicore-Windows-X64.zip"
    echo ""
fi

if [ -f "$RELEASE_DIR/magicore-Windows-ARM64.zip" ]; then
    echo "Windows ARM64:"
    shasum -a 256 "$RELEASE_DIR/magicore-Windows-ARM64.zip"
    echo ""
fi

echo "✅ Hashes computed. Update packaging/homebrew/magicore.rb and packaging/scoop/magicore.json"
