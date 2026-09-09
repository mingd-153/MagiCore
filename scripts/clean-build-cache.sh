#!/usr/bin/env bash
# Clean local cargo build disk pressure — the contributor-friendly
# alternative to forcing build.incremental=false on everyone.
# Dọn áp lực disk build local — thay thế thân thiện cho contributor thay
# vì ép build.incremental=false cho toàn bộ mọi người.
#
# Usage: bash scripts/clean-build-cache.sh [--all]
#   default: remove incremental caches only (safe, next build slower)
#   --all:   also dedupe stale compiled artifacts (keeps newest per crate)

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="$ROOT/target"

free_gb() { df -h . | awk 'NR==2 {print $4}'; }

before=$(free_gb)

if [ -d "$TARGET/debug/incremental" ]; then
  size=$(du -sh "$TARGET/debug/incremental" 2>/dev/null | cut -f1)
  rm -rf "$TARGET/debug/incremental"
  echo "removed incremental cache: $size"
fi

if [ "${1:-}" = "--all" ]; then
  # Keep the newest artifact per crate, drop stale hashes.
  for ext in rlib rmeta; do
    if [ -d "$TARGET/debug/deps" ]; then
      (cd "$TARGET/debug/deps" && \
        ls -t *.$ext 2>/dev/null | awk -F'-' \
        '{name=$0; sub(/-[0-9a-f]{16}\.'"$ext"'$/,"",name); if(seen[name]++) print $0}' \
        | xargs rm -f 2>/dev/null || true)
    fi
  done
  echo "deduped stale rlib/rmeta artifacts"
fi

after=$(free_gb)
echo "disk free: ${before}G -> ${after}G"
