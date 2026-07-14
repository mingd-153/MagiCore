#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAB_DIR="$ROOT/tools/core-web-lab"
STAMP="$(date '+%Y%m%d-%H%M%S')"
OUT="$LAB_DIR/reports/tests-$STAMP.md"

mkdir -p "$LAB_DIR/reports"

{
  echo "# Core-Web Test Lane"
  echo
  echo "- timestamp: $STAMP"
  echo
  echo "## Adapter"
  echo
  echo '```text'
  cargo test -p mg-web-adapter
  echo '```'
  echo
  echo "## CLI Core-Web"
  echo
  echo '```text'
  cargo test -p mg core::web
  echo '```'
} > "$OUT"

cargo test -p mg-web-adapter | tee -a "$OUT"
echo >> "$OUT"
cargo test -p mg core::web | tee -a "$OUT"
