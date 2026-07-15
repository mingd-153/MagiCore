#!/usr/bin/env bash
# Install mg binary from GitHub release.
# Usage: curl -fsSL https://raw.githubusercontent.com/mingd-153/MegaGate/main/scripts/install-from-gh.sh | bash
# Or:  ./scripts/install-from-gh.sh [--version v0.1.0] [--dir /usr/local/bin]
set -euo pipefail

REPO="mingd-153/MegaGate"
VERSION="latest"
INSTALL_DIR="/usr/local/bin"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --dir) INSTALL_DIR="$2"; shift 2 ;;
    *) echo "Usage: $0 [--version vX.Y.Z] [--dir /usr/local/bin]"; exit 1 ;;
  esac
done

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  linux)  OS_LABEL="Linux" ;;
  darwin) OS_LABEL="macOS" ;;
  mingw*|msys*) OS_LABEL="Windows";;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH_LABEL="X64" ;;
  aarch64|arm64) ARCH_LABEL="ARM64" ;;
  *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

PKG="megagate-web"
LABEL="${PKG}-${OS_LABEL}-${ARCH_LABEL}"

if [ "$VERSION" = "latest" ]; then
  URL="https://github.com/${REPO}/releases/latest/download/${LABEL}.tar.gz"
else
  URL="https://github.com/${REPO}/releases/download/${VERSION}/${LABEL}.tar.gz"
fi

echo "Downloading ${LABEL} from ${URL} ..."
TMP=$(mktemp -d)
curl -fsSL "$URL" | tar xz -C "$TMP"

if [ -f "$TMP/mg" ]; then
  BIN="$TMP/mg"
elif [ -f "$TMP/${PKG}/mg" ]; then
  BIN="$TMP/${PKG}/mg"
else
  echo "Error: mg binary not found in archive"
  find "$TMP" -type f
  exit 1
fi

install -d "$INSTALL_DIR"
install "$BIN" "$INSTALL_DIR/mg"
rm -rf "$TMP"

echo "Installed mg -> ${INSTALL_DIR}/mg"
echo "Run 'mg --help' to verify."
