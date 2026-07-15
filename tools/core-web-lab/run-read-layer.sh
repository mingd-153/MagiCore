#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAB_DIR="$ROOT/tools/core-web-lab"
STAMP="$(date '+%Y%m%d-%H%M%S')"
FILES_MANIFEST="$LAB_DIR/manifests/core-web-files.txt"
SUMMARY="$LAB_DIR/indexes/read-layer-$STAMP.md"
HASHES="$LAB_DIR/indexes/core-web-hashes-$STAMP.txt"
SIZES="$LAB_DIR/indexes/core-web-sizes-$STAMP.txt"
TARGETS="$LAB_DIR/manifests/repograph-targets.txt"

"$LAB_DIR/bootstrap.sh" >/dev/null

if [[ ! -f "$FILES_MANIFEST" ]]; then
  echo "missing manifest: $FILES_MANIFEST" >&2
  exit 1
fi

cp "$FILES_MANIFEST" "$TARGETS"

: > "$HASHES"
: > "$SIZES"

while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  if [[ -f "$ROOT/$file" ]]; then
    shasum -a 256 "$ROOT/$file" >> "$HASHES"
    wc -lc "$ROOT/$file" >> "$SIZES"
  fi
done < "$FILES_MANIFEST"

{
  echo "# Core-Web Read Layer"
  echo
  echo "- timestamp: $STAMP"
  echo "- manifest: $(basename "$FILES_MANIFEST")"
  echo "- hash index: $(basename "$HASHES")"
  echo "- size index: $(basename "$SIZES")"
  echo
  echo "## External tool readiness"
  echo
  if command -v repograph >/dev/null 2>&1; then
    echo "- RepoGraph: installed"
  else
    echo "- RepoGraph: not installed yet"
  fi
  if command -v opengrok >/dev/null 2>&1; then
    echo "- OpenGrok: installed"
  else
    echo "- OpenGrok: not installed yet"
  fi
  if command -v zoekt-index >/dev/null 2>&1; then
    echo "- Zoekt: installed"
  else
    echo "- Zoekt: not installed yet"
  fi
} > "$SUMMARY"

echo "read-layer artifacts written"
echo "summary: $SUMMARY"
