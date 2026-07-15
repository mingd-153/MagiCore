#!/usr/bin/env bash
set -euo pipefail

# MegaGate local install script
# Usage: ./scripts/install.sh [--prefix /usr/local] [--package megagate-web]

PREFIX="${PREFIX:-/usr/local}"
PKG="${PACKAGE:-megagate-web}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) PREFIX="$2"; shift 2 ;;
    --package) PKG="$2"; shift 2 ;;
    *) echo "Usage: $0 [--prefix <dir>] [--package <name>]"; exit 1 ;;
  esac
done

echo "Building $PKG ..."
cd "$(dirname "$0")/.."
cargo run -p mg-dist -- build "$PKG"

TARGET_DIR="dist/$PKG"
TARGET_SUBDIR=$(ls "$TARGET_DIR" 2>/dev/null | head -1)
if [[ -z "$TARGET_SUBDIR" ]]; then
  echo "Error: no build artifact found in $TARGET_DIR"
  exit 1
fi

BINARY="$TARGET_DIR/$TARGET_SUBDIR/mg"
if [[ ! -f "$BINARY" ]]; then
  echo "Error: binary not found at $BINARY"
  exit 1
fi

install -d "$PREFIX/bin"
install "$BINARY" "$PREFIX/bin/mg"
echo "Installed mg -> $PREFIX/bin/mg"
echo "Run 'mg --help' to verify."
