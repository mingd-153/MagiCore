#!/usr/bin/env bash
# Release Artifact Naming Contract / Quy ước đặt tên artifact phát hành
# Single source of truth for artifact names / Nguồn chân lý duy nhất cho tên artifact

set -euo pipefail

VERSION="${1:-}"
OS="${2:-}"
ARCH="${3:-}"
VARIANT="${4:-all}"  # all or web / all hoặc web

if [[ -z "$VERSION" || -z "$OS" || -z "$ARCH" ]]; then
    echo "Usage: $0 VERSION OS ARCH [VARIANT]" >&2
    echo "Example: $0 1.1.0 linux x64 web" >&2
    exit 1
fi

# Validate inputs / Kiểm tra đầu vào
case "$OS" in
    linux|macos|windows) ;;
    *) echo "Error: Invalid OS '$OS'. Must be: linux, macos, windows" >&2; exit 1 ;;
esac

case "$ARCH" in
    x64|arm64) ;;
    *) echo "Error: Invalid ARCH '$ARCH'. Must be: x64, arm64" >&2; exit 1 ;;
esac

case "$VARIANT" in
    all|web) ;;
    *) echo "Error: Invalid VARIANT '$VARIANT'. Must be: all, web" >&2; exit 1 ;;
esac

# Determine extension / Xác định phần mở rộng
case "$OS" in
    windows) EXT="zip" ;;
    *) EXT="tar.gz" ;;
esac

# Standardized naming: magicore[-web]-VERSION-OS-ARCH.EXT
# Quy ước chuẩn: magicore[-web]-VERSION-OS-ARCH.EXT
PREFIX="magicore"
if [[ "$VARIANT" == "web" ]]; then
    PREFIX="magicore-web"
fi

ARCHIVE_NAME="${PREFIX}-${VERSION}-${OS}-${ARCH}.${EXT}"
CHECKSUM_NAME="${ARCHIVE_NAME}.sha256"
SBOM_NAME="${PREFIX}-${VERSION}-${OS}-${ARCH}-sbom.json"

# Output in parseable format / Xuất định dạng có thể parse
echo "ARCHIVE=${ARCHIVE_NAME}"
echo "CHECKSUM=${CHECKSUM_NAME}"
echo "SBOM=${SBOM_NAME}"
echo "VARIANT=${VARIANT}"
echo "OS=${OS}"
echo "ARCH=${ARCH}"
echo "EXT=${EXT}"
