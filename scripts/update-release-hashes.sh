#!/usr/bin/env bash
set -euo pipefail

# Release hash updater - computes SHA256 values from real release artifacts.
# Cong cu cap nhat hash release - chi dung artifact that, khong tu tao hash gia.

usage() {
  cat <<'USAGE'
Usage: scripts/update-release-hashes.sh --artifacts <dir> --version <version> [--verify-only]

Options:
  --artifacts <dir>   Directory containing release artifacts
  --version <version> Release version (e.g. 1.1.0-rc.2.1)
  --verify-only       Only verify existing hashes, do not update
  -h, --help          Show this help

The command fails when an expected artifact is missing.
USAGE
}

artifacts_dir=""
version=""
verify_only=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifacts)
      if [[ $# -lt 2 ]]; then
        echo "error: --artifacts requires a directory" >&2
        exit 2
      fi
      artifacts_dir="$2"
      shift 2
      ;;
    --version)
      if [[ $# -lt 2 ]]; then
        echo "error: --version requires a value" >&2
        exit 2
      fi
      version="$2"
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

if [[ -z "$artifacts_dir" ]]; then
  echo "error: --artifacts is required" >&2
  usage >&2
  exit 2
fi

if [[ -z "$version" ]]; then
  echo "error: --version is required" >&2
  usage >&2
  exit 2
fi

if [[ ! -d "$artifacts_dir" ]]; then
  echo "error: artifact directory does not exist: $artifacts_dir" >&2
  exit 2
fi

# Repo root override - lets tests verify updates without touching real manifests.
# Override repo root - cho phep test update tren ban copy, khong cham manifest that.
repo_root="${MAGICORE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
homebrew_dir="$repo_root/packaging/homebrew"
scoop_dir="$repo_root/packaging/scoop"

sha256_file() {
  local file="$1"

  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$file" | awk '{print $2}'
  else
    echo "error: no SHA256 tool found (need shasum, sha256sum, or openssl)" >&2
    exit 2
  fi
}

artifact_hash() {
  local artifact="$1"
  local file="$artifacts_dir/$artifact"

  if [[ ! -f "$file" ]]; then
    echo "error: missing release artifact: $file" >&2
    exit 2
  fi

  sha256_file "$file"
}

homebrew_match_count() {
  local file="$1"
  local artifact="$2"
  ARTIFACT="$artifact" perl -0ne '
    my $artifact = quotemeta($ENV{ARTIFACT});
    my @matches = /url "[^"]*\/$artifact"\n\s*sha256 "[^"]*"/g;
    print scalar @matches;
  ' "$file"
}

homebrew_expected_count() {
  local file="$1"
  local artifact="$2"
  local sha="$3"
  ARTIFACT="$artifact" SHA="$sha" perl -0ne '
    my $artifact = quotemeta($ENV{ARTIFACT});
    my $sha = quotemeta($ENV{SHA});
    my @matches = /url "[^"]*\/$artifact"\n\s*sha256 "$sha"/g;
    print scalar @matches;
  ' "$file"
}

scoop_match_count() {
  local file="$1"
  local artifact="$2"
  ARTIFACT="$artifact" perl -0ne '
    my $artifact = quotemeta($ENV{ARTIFACT});
    my @matches = /"url": "[^"]*\/$artifact",\n\s*"hash": "[^"]*"/g;
    print scalar @matches;
  ' "$file"
}

scoop_expected_count() {
  local file="$1"
  local artifact="$2"
  local sha="$3"
  ARTIFACT="$artifact" SHA="$sha" perl -0ne '
    my $artifact = quotemeta($ENV{ARTIFACT});
    my $sha = quotemeta($ENV{SHA});
    my @matches = /"url": "[^"]*\/$artifact",\n\s*"hash": "$sha"/g;
    print scalar @matches;
  ' "$file"
}

replace_homebrew_hash() {
  local file="$1"
  local artifact="$2"
  local sha="$3"
  local count

  count="$(homebrew_match_count "$file" "$artifact")"
  if [[ "$count" != "1" ]]; then
    echo "error: expected one Homebrew entry for $artifact in $file, found $count" >&2
    exit 2
  fi

  ARTIFACT="$artifact" SHA="$sha" perl -0pi -e '
    my $artifact = quotemeta($ENV{ARTIFACT});
    my $sha = $ENV{SHA};
    s|(url "[^"]*/$artifact"\n\s*sha256 )"[^"]*"|$1"$sha"|g;
  ' "$file"
}

replace_scoop_hash() {
  local file="$1"
  local artifact="$2"
  local sha="$3"
  local count

  count="$(scoop_match_count "$file" "$artifact")"
  if [[ "$count" != "1" ]]; then
    echo "error: expected one Scoop entry for $artifact in $file, found $count" >&2
    exit 2
  fi

  ARTIFACT="$artifact" SHA="$sha" perl -0pi -e '
    my $artifact = quotemeta($ENV{ARTIFACT});
    my $sha = $ENV{SHA};
    s|("url": "[^"]*/$artifact",\n\s*"hash": )"[^"]*"|$1"$sha"|g;
  ' "$file"
}

verify_homebrew_hash() {
  local file="$1"
  local artifact="$2"
  local sha="$3"
  local count

  count="$(homebrew_expected_count "$file" "$artifact" "$sha")"
  if [[ "$count" != "1" ]]; then
    echo "error: Homebrew hash is not current for $artifact in $file" >&2
    exit 1
  fi
}

verify_scoop_hash() {
  local file="$1"
  local artifact="$2"
  local sha="$3"
  local count

  count="$(scoop_expected_count "$file" "$artifact" "$sha")"
  if [[ "$count" != "1" ]]; then
    echo "error: Scoop hash is not current for $artifact in $file" >&2
    exit 1
  fi
}

all_core_homebrew="$homebrew_dir/magicore.rb"
web_core_homebrew="$homebrew_dir/magicore-web.rb"
all_core_scoop="$scoop_dir/magicore.json"
web_core_scoop="$scoop_dir/magicore-web.json"

# New naming pattern: magicore-{version}-{os}-{arch}.{ext}
# Only x86_64 targets (ARM64 removed for RC-2)
magicore_linux_x64_hash="$(artifact_hash "magicore-${version}-linux-x64.tar.gz")"
magicore_macos_x64_hash="$(artifact_hash "magicore-${version}-macos-x64.tar.gz")"
magicore_windows_x64_hash="$(artifact_hash "magicore-${version}-windows-x64.zip")"
magicore_web_linux_x64_hash="$(artifact_hash "magicore-web-${version}-linux-x64.tar.gz")"
magicore_web_macos_x64_hash="$(artifact_hash "magicore-web-${version}-macos-x64.tar.gz")"
magicore_web_windows_x64_hash="$(artifact_hash "magicore-web-${version}-windows-x64.zip")"

if [[ "$verify_only" -eq 1 ]]; then
  verify_homebrew_hash "$all_core_homebrew" "magicore-${version}-macos-x64.tar.gz" "$magicore_macos_x64_hash"
  verify_homebrew_hash "$all_core_homebrew" "magicore-${version}-linux-x64.tar.gz" "$magicore_linux_x64_hash"
  verify_scoop_hash "$all_core_scoop" "magicore-${version}-windows-x64.zip" "$magicore_windows_x64_hash"

  verify_homebrew_hash "$web_core_homebrew" "magicore-web-${version}-macos-x64.tar.gz" "$magicore_web_macos_x64_hash"
  verify_homebrew_hash "$web_core_homebrew" "magicore-web-${version}-linux-x64.tar.gz" "$magicore_web_linux_x64_hash"
  verify_scoop_hash "$web_core_scoop" "magicore-web-${version}-windows-x64.zip" "$magicore_web_windows_x64_hash"

  if grep -R "UPDATE_ME\|0000000000000000" "$homebrew_dir" "$scoop_dir" >/dev/null 2>&1; then
    echo "error: packaging still contains UPDATE_ME or zero-hash placeholders" >&2
    exit 1
  fi

  if grep -R "PLACEHOLDER_WILL_BE_REPLACED_BY_CI" "$homebrew_dir" "$scoop_dir" >/dev/null 2>&1; then
    echo "error: packaging still contains PLACEHOLDER_WILL_BE_REPLACED_BY_CI" >&2
    echo "       All placeholders should have been replaced with real hashes" >&2
    exit 1
  fi

  echo "Release hashes are current."
  exit 0
fi

replace_homebrew_hash "$all_core_homebrew" "magicore-${version}-macos-x64.tar.gz" "$magicore_macos_x64_hash"
replace_homebrew_hash "$all_core_homebrew" "magicore-${version}-linux-x64.tar.gz" "$magicore_linux_x64_hash"
replace_scoop_hash "$all_core_scoop" "magicore-${version}-windows-x64.zip" "$magicore_windows_x64_hash"

replace_homebrew_hash "$web_core_homebrew" "magicore-web-${version}-macos-x64.tar.gz" "$magicore_web_macos_x64_hash"
replace_homebrew_hash "$web_core_homebrew" "magicore-web-${version}-linux-x64.tar.gz" "$magicore_web_linux_x64_hash"
replace_scoop_hash "$web_core_scoop" "magicore-web-${version}-windows-x64.zip" "$magicore_web_windows_x64_hash"

echo "Release hashes updated from $artifacts_dir for version $version."
