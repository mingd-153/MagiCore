#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAB_DIR="$ROOT/tools/core-web-lab"
STAMP="$(date '+%Y%m%d-%H%M%S')"

mkdir -p \
  "$LAB_DIR/graph" \
  "$LAB_DIR/indexes" \
  "$LAB_DIR/manifests" \
  "$LAB_DIR/reports" \
  "$LAB_DIR/benchmarks" \
  "$LAB_DIR/security"

BRANCH="$(git -C "$ROOT" branch --show-current || true)"
COMMIT="$(git -C "$ROOT" rev-parse --short HEAD || true)"

{
  echo "timestamp=$STAMP"
  echo "branch=$BRANCH"
  echo "commit=$COMMIT"
  echo "root=$ROOT"
} > "$LAB_DIR/reports/bootstrap-$STAMP.env"

rg --files "$ROOT/adapters/web" "$ROOT/cli/src" "$ROOT/templates/web" "$ROOT/scripts" \
  | sed "s#^$ROOT/##" \
  | rg '(^adapters/web/|^cli/src/commands/core/web\.rs$|^cli/src/wizard/web\.rs$|^templates/web/|^scripts/bench\.sh$|^scripts/bench-ci\.sh$)' \
  > "$LAB_DIR/manifests/core-web-files.txt"

{
  echo "# Core-Web Surface Snapshot"
  echo
  echo "- timestamp: $STAMP"
  echo "- branch: $BRANCH"
  echo "- commit: $COMMIT"
  echo "- files: $(wc -l < "$LAB_DIR/manifests/core-web-files.txt" | tr -d ' ')"
} > "$LAB_DIR/reports/bootstrap-$STAMP.md"

echo "core-web lab bootstrapped"
echo "manifest: $LAB_DIR/manifests/core-web-files.txt"
echo "report:   $LAB_DIR/reports/bootstrap-$STAMP.md"
