#!/bin/sh
# SECURITY: This script downloads and executes code from the internet.
# To verify the integrity of this script, use GPG:
#   curl -fsSL https://mgpm.dev/install.sh | gpg --verify
# Or verify the SHA-256 checksum:
#   curl -fsSL https://mgpm.dev/install.sh | sha256sum
# MGPM - MegaGate Package Manager
# https://mgpm.dev/install.sh

set -eu

MGPM_VERSION="${MGPM_VERSION:-latest}"
MGPM_DIR="${MGPM_DIR:-$HOME/.mgpm/bin}"

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  linux) TARGET="$ARCH-unknown-linux-gnu" ;;
  darwin) TARGET="$ARCH-apple-darwin" ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

# Determine latest version
if [ "$MGPM_VERSION" = "latest" ]; then
  MGPM_VERSION=$(curl -fsSL https://api.github.com/repos/megagate/mgpm/releases/latest | grep '"tag_name"' | cut -d'"' -f4)
fi

# Download
URL="https://github.com/megagate/mgpm/releases/download/$MGPM_VERSION/mgpm-$TARGET.tar.gz"
echo "Downloading mgpm $MGPM_VERSION for $TARGET..."

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

cd "$TMPDIR"
curl -fsSLO "$URL"
curl -fsSLO "$URL.sha256"
curl -fsSLO "$URL.asc"

# Verify SHA-256
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum -c "mgpm-$TARGET.tar.gz.sha256" || { echo "⚠ SHA-256 mismatch!"; exit 1; }
elif command -v shasum >/dev/null 2>&1; then
  shasum -a 256 -c "mgpm-$TARGET.tar.gz.sha256" || { echo "⚠ SHA-256 mismatch!"; exit 1; }
fi

# Verify GPG signature
if command -v gpg >/dev/null 2>&1; then
  echo "Verifying GPG signature..."
  gpg --keyserver keys.openpgp.org --recv-keys 0xMGPMKEYID 2>/dev/null || true
  if gpg --verify "mgpm-$TARGET.tar.gz.asc" "mgpm-$TARGET.tar.gz" 2>/dev/null; then
    echo "✓ GPG signature verified"
  else
    echo "⚠ GPG signature verification failed (proceeding anyway)"
  fi
fi

mkdir -p "$MGPM_DIR"
tar xzf "mgpm-$TARGET.tar.gz" -C "$MGPM_DIR"

# Add to PATH if not already
case "$SHELL" in
  */zsh) PROFILE="$HOME/.zshrc" ;;
  */bash) PROFILE="$HOME/.bashrc" ;;
  *) PROFILE="$HOME/.profile" ;;
esac

if ! echo ":$PATH:" | grep -q ":$MGPM_DIR:"; then
  echo "export PATH=\"\$PATH:$MGPM_DIR\"" >> "$PROFILE"
  echo "Added $MGPM_DIR to PATH in $PROFILE"
fi

echo "mgpm $MGPM_VERSION installed successfully!"
echo "Run 'mgpm --help' to get started."
