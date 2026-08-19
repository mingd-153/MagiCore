#!/usr/bin/env bash
# Install mg binary from GitHub release.
# Usage: curl -fsSL https://raw.githubusercontent.com/mingd-153/MegaGate/main/scripts/install-from-gh.sh | bash
# Or:  ./scripts/install-from-gh.sh [--package megagate-web] [--version v0.1.0] [--dir /usr/local/bin]
# Local test: ./scripts/install-from-gh.sh --archive dist/megagate-web-macOS-ARM64.tar.gz --dir /tmp/mg-bin
set -euo pipefail

REPO="mingd-153/MegaGate"
PKG="megagate-web"
VERSION="latest"
INSTALL_DIR="/usr/local/bin"
ARCHIVE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --package) PKG="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --dir) INSTALL_DIR="$2"; shift 2 ;;
    --archive) ARCHIVE="$2"; shift 2 ;;
    *) echo "Usage: $0 [--package megagate-web|megagate] [--version vX.Y.Z] [--dir /usr/local/bin] [--archive <tar.gz>]"; exit 1 ;;
  esac
done

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

verify_checksum() {
  archive="$1"
  checksum_file="$2"
  expected="$(awk '{print $1}' "$checksum_file")"
  actual="$(sha256_file "$archive")"
  if [ "$expected" != "$actual" ]; then
    echo "Error: checksum mismatch for $archive"
    echo "Expected: $expected"
    echo "Actual:   $actual"
    exit 1
  fi
}

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  linux)  OS_LABEL="Linux"; EXE_SUFFIX="" ;;
  darwin) OS_LABEL="macOS"; EXE_SUFFIX="" ;;
  mingw*|msys*) OS_LABEL="Windows"; EXE_SUFFIX=".exe" ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH_LABEL="X64" ;;
  aarch64|arm64) ARCH_LABEL="ARM64" ;;
  *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

ARCHIVE_PATH="$TMP/archive.tar.gz"
CHECKSUM_PATH="$TMP/archive.tar.gz.sha256"

if [ -n "$ARCHIVE" ]; then
  echo "Installing ${PKG} from local archive ${ARCHIVE} ..."
  cp "$ARCHIVE" "$ARCHIVE_PATH"
  if [ -f "${ARCHIVE}.sha256" ]; then
    cp "${ARCHIVE}.sha256" "$CHECKSUM_PATH"
  else
    echo "Error: checksum file not found: ${ARCHIVE}.sha256"
    exit 1
  fi
else
  LABEL="${PKG}-${OS_LABEL}-${ARCH_LABEL}"
  if [ "$VERSION" = "latest" ]; then
    URL="https://github.com/${REPO}/releases/latest/download/${LABEL}.tar.gz"
  else
    URL="https://github.com/${REPO}/releases/download/${VERSION}/${LABEL}.tar.gz"
  fi

  echo "Downloading ${LABEL} from ${URL} ..."
  curl -fsSL "$URL" -o "$ARCHIVE_PATH"
  curl -fsSL "${URL}.sha256" -o "$CHECKSUM_PATH"
fi

verify_checksum "$ARCHIVE_PATH" "$CHECKSUM_PATH"
tar xzf "$ARCHIVE_PATH" -C "$TMP"

if [ -f "$TMP/mg${EXE_SUFFIX}" ]; then
  BIN="$TMP/mg${EXE_SUFFIX}"
elif [ -f "$TMP/${PKG}/mg${EXE_SUFFIX}" ]; then
  BIN="$TMP/${PKG}/mg${EXE_SUFFIX}"
else
  echo "Error: mg binary not found in archive"
  find "$TMP" -type f
  exit 1
fi

mkdir -p "$INSTALL_DIR"
if command -v install >/dev/null 2>&1; then
  install "$BIN" "$INSTALL_DIR/mg${EXE_SUFFIX}"
else
  cp "$BIN" "$INSTALL_DIR/mg${EXE_SUFFIX}"
  chmod +x "$INSTALL_DIR/mg${EXE_SUFFIX}" 2>/dev/null || true
fi

echo "Installed mg -> ${INSTALL_DIR}/mg${EXE_SUFFIX}"
echo "Run 'mg --help' to verify."
