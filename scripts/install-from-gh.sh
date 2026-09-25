#!/usr/bin/env bash
# Install mgc binary from GitHub release.
# Usage: curl -fsSL https://raw.githubusercontent.com/mingd-153/MagiCore/main/scripts/install-from-gh.sh | bash
# Or:  ./scripts/install-from-gh.sh [--package magicore|magicore-web] [--version v0.1.0] [--dir /usr/local/bin]
# Local test: ./scripts/install-from-gh.sh --archive dist/magicore-web-1.1.0-rc.6-macos-arm64.tar.gz --dir /tmp/mgc-bin
set -euo pipefail

REPO="mingd-153/MagiCore"
PKG="magicore"
VERSION="latest"
INSTALL_DIR="/usr/local/bin"
ARCHIVE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --package) PKG="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --dir) INSTALL_DIR="$2"; shift 2 ;;
    --archive) ARCHIVE="$2"; shift 2 ;;
    *) echo "Usage: $0 [--package magicore-web|magicore] [--version vX.Y.Z] [--dir /usr/local/bin] [--archive <tar.gz>]"; exit 1 ;;
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

# Contract labels are lowercase (scripts/release-artifact-contract.sh is
# the single source of truth): linux/macos/windows + x64/arm64.
case "$OS" in
  linux)  OS_LABEL="linux"; EXE_SUFFIX=""; EXT="tar.gz" ;;
  darwin) OS_LABEL="macos"; EXE_SUFFIX=""; EXT="tar.gz" ;;
  mingw*|msys*) OS_LABEL="windows"; EXE_SUFFIX=".exe"; EXT="zip" ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH_LABEL="x64" ;;
  aarch64|arm64) ARCH_LABEL="arm64" ;;
  *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

# Resolve "latest" without a JSON parser: /releases/latest redirects to
# /releases/tag/<tag> — the effective URL carries the tag.
resolve_latest_tag() {
  local final_url
  final_url="$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest")" || return 1
  local tag="${final_url##*/tag/}"
  if [[ -z "$tag" || "$tag" == "$final_url" ]]; then return 1; fi
  printf '%s' "$tag"
}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

ARCHIVE_PATH="$TMP/archive.pkg"
CHECKSUM_PATH="$TMP/archive.pkg.sha256"

if [ -n "$ARCHIVE" ]; then
  echo "Installing ${PKG} from local archive ${ARCHIVE} ..."
  cp "$ARCHIVE" "$ARCHIVE_PATH"
  case "$ARCHIVE" in
    *.zip) EXT="zip" ;;
    *) EXT="tar.gz" ;;
  esac
  if [ -f "${ARCHIVE}.sha256" ]; then
    cp "${ARCHIVE}.sha256" "$CHECKSUM_PATH"
  else
    echo "Error: checksum file not found: ${ARCHIVE}.sha256"
    exit 1
  fi
else
  # Asset names follow scripts/release-artifact-contract.sh:
  # {package}-{version}-{os}-{arch}.{ext}, all lowercase, version WITHOUT
  # the leading 'v' (e.g. magicore-1.1.0-rc.6-macos-arm64.tar.gz).
  if [ "$VERSION" = "latest" ]; then
    if ! TAG="$(resolve_latest_tag)"; then
      echo "Error: could not resolve the latest release (network/API unreachable)."
      echo "Re-run with an explicit --version (e.g. --version 1.1.0-rc.6)."
      exit 1
    fi
    VERSION="$TAG"
  fi
  VERSION_NUMBER="${VERSION#v}"
  if ! [[ "$VERSION_NUMBER" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
    echo "Error: invalid version: $VERSION (expected like 1.1.0-rc.6, with or without a leading 'v')."
    exit 1
  fi
  LABEL="${PKG}-${VERSION_NUMBER}-${OS_LABEL}-${ARCH_LABEL}"
  URL="https://github.com/${REPO}/releases/download/v${VERSION_NUMBER}/${LABEL}.${EXT}"

  echo "Downloading ${LABEL}.${EXT} from ${URL} ..."
  curl -fsSL "$URL" -o "$ARCHIVE_PATH"
  curl -fsSL "${URL}.sha256" -o "$CHECKSUM_PATH"
fi

verify_checksum "$ARCHIVE_PATH" "$CHECKSUM_PATH"
if [ "$EXT" = "zip" ]; then
  if ! command -v unzip >/dev/null 2>&1; then
    echo "Error: .zip archive requires 'unzip' (not found in PATH)."
    exit 1
  fi
  unzip -o -q "$ARCHIVE_PATH" -d "$TMP"
else
  tar xzf "$ARCHIVE_PATH" -C "$TMP"
fi

if [ -f "$TMP/mgc${EXE_SUFFIX}" ]; then
  BIN="$TMP/mgc${EXE_SUFFIX}"
elif [ -f "$TMP/${PKG}/mgc${EXE_SUFFIX}" ]; then
  BIN="$TMP/${PKG}/mgc${EXE_SUFFIX}"
else
  echo "Error: mgc binary not found in archive"
  find "$TMP" -type f
  exit 1
fi

mkdir -p "$INSTALL_DIR"
if command -v install >/dev/null 2>&1; then
  install "$BIN" "$INSTALL_DIR/mgc${EXE_SUFFIX}"
else
  cp "$BIN" "$INSTALL_DIR/mgc${EXE_SUFFIX}"
  chmod +x "$INSTALL_DIR/mgc${EXE_SUFFIX}" 2>/dev/null || true
fi

echo "Installed mgc -> ${INSTALL_DIR}/mgc${EXE_SUFFIX}"
echo "Run 'mgc --help' to verify."
