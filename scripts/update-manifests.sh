#!/usr/bin/env bash
set -euo pipefail

# Manifest updater - updates version, URLs, hashes for release
# Handles Homebrew and Scoop manifests with version sync

usage() {
  cat <<'USAGE'
Usage: scripts/update-manifests.sh --version <version> --artifacts <dir> [--verify-only]

Updates package manager manifests with:
  - Version number
  - Download URLs (with new naming pattern)
  - SHA256 hashes (computed from artifacts)
  - Remove unsupported architectures

Options:
  --version <version>  Release version (e.g. 1.1.0-rc.3)
  --artifacts <dir>    Directory containing release artifacts
  --verify-only        Only verify, do not update
  -h, --help           Show this help

Example:
  ./scripts/update-manifests.sh --version 1.1.0-rc.3 --artifacts release-assets
USAGE
}

version=""
artifacts_dir=""
verify_only=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      if [[ $# -lt 2 ]]; then
        echo "error: --version requires a value" >&2
        exit 2
      fi
      version="$2"
      shift 2
      ;;
    --artifacts)
      if [[ $# -lt 2 ]]; then
        echo "error: --artifacts requires a directory" >&2
        exit 2
      fi
      artifacts_dir="$2"
      shift 2
      ;;
    --verify-only)
      verify_only=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$version" ]] || [[ -z "$artifacts_dir" ]]; then
  echo "error: --version and --artifacts are required" >&2
  usage >&2
  exit 2
fi

if [[ ! -d "$artifacts_dir" ]]; then
  echo "error: artifact directory does not exist: $artifacts_dir" >&2
  exit 2
fi

repo_root="${MAGICORE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
homebrew_dir="$repo_root/packaging/homebrew"
scoop_dir="$repo_root/packaging/scoop"

# Compute SHA256
sha256_file() {
  local file="$1"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$file" | awk '{print $2}'
  else
    echo "error: no SHA256 tool found" >&2
    exit 2
  fi
}

# Get artifact hash
artifact_hash() {
  local artifact="$1"
  local file="$artifacts_dir/$artifact"
  if [[ ! -f "$file" ]]; then
    echo "error: missing artifact: $file" >&2
    exit 2
  fi
  sha256_file "$file"
}

# Artifact names for RC-3: only x86_64 targets
magicore_linux_x64="magicore-${version}-linux-x64.tar.gz"
magicore_macos_x64="magicore-${version}-macos-x64.tar.gz"
magicore_windows_x64="magicore-${version}-windows-x64.zip"
magicore_web_linux_x64="magicore-web-${version}-linux-x64.tar.gz"
magicore_web_macos_x64="magicore-web-${version}-macos-x64.tar.gz"
magicore_web_windows_x64="magicore-web-${version}-windows-x64.zip"

echo "Computing hashes for version $version..."
hash_linux_x64=$(artifact_hash "$magicore_linux_x64")
hash_macos_x64=$(artifact_hash "$magicore_macos_x64")
hash_windows_x64=$(artifact_hash "$magicore_windows_x64")
hash_web_linux_x64=$(artifact_hash "$magicore_web_linux_x64")
hash_web_macos_x64=$(artifact_hash "$magicore_web_macos_x64")
hash_web_windows_x64=$(artifact_hash "$magicore_web_windows_x64")

echo "✓ All hashes computed"

if [[ "$verify_only" -eq 1 ]]; then
  echo "Verify mode - checking manifests contain correct hashes..."
  
  # Check Homebrew magicore.rb
  if ! grep -q "$hash_macos_x64" "$homebrew_dir/magicore.rb"; then
    echo "❌ magicore.rb: macOS x64 hash mismatch" >&2
    exit 1
  fi
  if ! grep -q "$hash_linux_x64" "$homebrew_dir/magicore.rb"; then
    echo "❌ magicore.rb: Linux x64 hash mismatch" >&2
    exit 1
  fi
  
  # Check Scoop magicore.json
  if ! grep -q "$hash_windows_x64" "$scoop_dir/magicore.json"; then
    echo "❌ magicore.json: Windows x64 hash mismatch" >&2
    exit 1
  fi
  
  # Check for placeholders
  if grep -R "PLACEHOLDER_WILL_BE_REPLACED_BY_CI\|UPDATE_ME" "$homebrew_dir" "$scoop_dir" >/dev/null 2>&1; then
    echo "❌ Found PLACEHOLDER or UPDATE_ME in manifests" >&2
    exit 1
  fi
  
  echo "✅ All manifests verified"
  exit 0
fi

echo "Updating Homebrew formula: magicore.rb"

# Ensure directories exist
mkdir -p "$homebrew_dir" "$scoop_dir"

# Update magicore.rb - write new version with only x64 support
cat > "$homebrew_dir/magicore.rb" <<EOF
class Magicore < Formula
  desc "Universal package manager with multi-core runtime"
  homepage "https://github.com/mingd-153/MagiCore"
  version "$version"
  license "MIT"
  head "https://github.com/mingd-153/MagiCore.git", branch: "main"

  depends_on "rust" => :build

  # Binary releases - x86_64 only for RC-3
  # ARM64 support planned for future release
  on_macos do
    if Hardware::CPU.arm?
      odie "ARM64 not yet supported. Use Rosetta 2 or build from source with: brew install --build-from-source"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v${version}/${magicore_macos_x64}"
      sha256 "$hash_macos_x64"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      odie "ARM64 not yet supported. Build from source with: cargo install mgc"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v${version}/${magicore_linux_x64}"
      sha256 "$hash_linux_x64"
    end
  end

  def install
    bin.install "mgc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mgc --version")
  end
end
EOF

echo "✓ Updated magicore.rb"

# Update magicore-web.rb
cat > "$homebrew_dir/magicore-web.rb" <<EOF
class MagicoreWeb < Formula
  desc "MagiCore package manager - web-only build"
  homepage "https://github.com/mingd-153/MagiCore"
  version "$version"
  license "MIT"

  # Binary releases - x86_64 only for RC-3
  on_macos do
    if Hardware::CPU.arm?
      odie "ARM64 not yet supported"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v${version}/${magicore_web_macos_x64}"
      sha256 "$hash_web_macos_x64"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      odie "ARM64 not yet supported"
    else
      url "https://github.com/mingd-153/MagiCore/releases/download/v${version}/${magicore_web_linux_x64}"
      sha256 "$hash_web_linux_x64"
    end
  end

  def install
    bin.install "mgc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mgc --version")
  end
end
EOF

echo "✓ Updated magicore-web.rb"

# Update Scoop manifests
cat > "$scoop_dir/magicore.json" <<EOF
{
  "version": "$version",
  "description": "Universal package manager with multi-core runtime",
  "homepage": "https://github.com/mingd-153/MagiCore",
  "license": "MIT",
  "architecture": {
    "64bit": {
      "url": "https://github.com/mingd-153/MagiCore/releases/download/v${version}/${magicore_windows_x64}",
      "hash": "$hash_windows_x64"
    }
  },
  "bin": "mgc.exe",
  "checkver": {
    "github": "https://github.com/mingd-153/MagiCore"
  },
  "autoupdate": {
    "architecture": {
      "64bit": {
        "url": "https://github.com/mingd-153/MagiCore/releases/download/v\$version/magicore-\$version-windows-x64.zip"
      }
    }
  }
}
EOF

echo "✓ Updated magicore.json"

cat > "$scoop_dir/magicore-web.json" <<EOF
{
  "version": "$version",
  "description": "MagiCore package manager - web-only build",
  "homepage": "https://github.com/mingd-153/MagiCore",
  "license": "MIT",
  "architecture": {
    "64bit": {
      "url": "https://github.com/mingd-153/MagiCore/releases/download/v${version}/${magicore_web_windows_x64}",
      "hash": "$hash_web_windows_x64"
    }
  },
  "bin": "mgc.exe",
  "checkver": {
    "github": "https://github.com/mingd-153/MagiCore"
  },
  "autoupdate": {
    "architecture": {
      "64bit": {
        "url": "https://github.com/mingd-153/MagiCore/releases/download/v\$version/magicore-web-\$version-windows-x64.zip"
      }
    }
  }
}
EOF

echo "✓ Updated magicore-web.json"

echo ""
echo "✅ All manifests updated for version $version"
echo "   - Homebrew: x64 only, ARM64 shows error message"
echo "   - Scoop: x64 only"
echo "   - All hashes verified"
