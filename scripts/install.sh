#!/usr/bin/env bash
set -euo pipefail

REPO="mingd-153/MegaGate"
PACKAGE="${MEGAGATE_PACKAGE:-megagate-web}"
VERSION="${MEGAGATE_VERSION:-latest}"
INSTALL_DIR="${MEGAGATE_INSTALL_DIR:-$HOME/.local/bin}"

detect_arch() {
  local arch
  arch=$(uname -m)
  case "$arch" in
    x86_64|amd64) echo "X64" ;;
    aarch64|arm64) echo "ARM64" ;;
    *) echo "unknown-$arch" ;;
  esac
}

detect_os() {
  local os
  os=$(uname -s)
  case "$os" in
    Linux) echo "Linux" ;;
    Darwin) echo "macOS" ;;
    MINGW*|MSYS*|CYGWIN*) echo "Windows" ;;
    *) echo "unknown-$os" ;;
  esac
}

fetch_release() {
  local pkg=$1 ver=$2 os=$3 arch=$4
  local asset="${pkg}-${os}-${arch}.tar.gz"

  if [ "$ver" = "latest" ]; then
    local url="https://github.com/${REPO}/releases/latest/download/${asset}"
  else
    local url="https://github.com/${REPO}/releases/download/${ver}/${asset}"
  fi

  echo "Downloading ${asset}..." >&2
  if command -v curl &>/dev/null; then
    curl -fSL "$url" -o "/tmp/${asset}"
  elif command -v wget &>/dev/null; then
    wget -q "$url" -O "/tmp/${asset}"
  else
    echo "Error: need curl or wget" >&2
    exit 1
  fi
  echo "/tmp/${asset}"
}

extract_and_install() {
  local archive=$1 dest=$2
  mkdir -p "$dest"
  tar xzf "$archive" -C "$dest"
  chmod +x "$dest/mg"
  echo "Installed mg to ${dest}/mg" >&2
}

add_to_path() {
  local dest=$1
  case "$SHELL" in
    *zsh*) rc="$HOME/.zshrc" ;;
    *bash*) rc="$HOME/.bashrc" ;;
    *) rc="" ;;
  esac
  if [ -n "$rc" ] && ! grep -q "export PATH=\"\$HOME/.local/bin:\$PATH\"" "$rc" 2>/dev/null; then
    echo "export PATH=\"\$HOME/.local/bin:\$PATH\"" >> "$rc"
    echo "Added ~/.local/bin to PATH in ${rc}" >&2
  fi
}

main() {
  local os arch archive
  os=$(detect_os)
  arch=$(detect_arch)

  echo "MegaGate ${VERSION} installer" >&2
  echo "  package: ${PACKAGE}" >&2
  echo "  os:      ${os}" >&2
  echo "  arch:    ${arch}" >&2
  echo "  dest:    ${INSTALL_DIR}" >&2
  echo "" >&2

  if [[ "$os" = unknown-* ]]; then
    echo "Error: unsupported OS: $(uname -s)" >&2
    exit 1
  fi

  archive=$(fetch_release "$PACKAGE" "$VERSION" "$os" "$arch")
  extract_and_install "$archive" "$INSTALL_DIR"
  rm -f "$archive"

  add_to_path "$INSTALL_DIR"

  echo "" >&2
  echo "MegaGate installed. Run:" >&2
  echo "  mg --help" >&2
}

main "$@"
