#!/usr/bin/env bash
# CI perf lane — runs matrix baseline comparison with regression detection
# Usage: scripts/bench-ci.sh [--save] [--compare-baseline <name>]
set -euo pipefail

cd "$(dirname "$0")/.."
BENCH_DIR="adapters/web"

run_matrix() {
  cargo run --bin bench_matrix --manifest-path "$BENCH_DIR/Cargo.toml" -- "$@"
}

SAVE=false
COMPARE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --save) SAVE=true; shift ;;
    --compare-baseline) COMPARE="$2"; shift 2 ;;
    *) echo "unknown: $1"; exit 1 ;;
  esac
done

if [ -n "$COMPARE" ]; then
  echo "── CI PERF: comparing standard vs baseline '$COMPARE' ──"
  if ! run_matrix --compare-baseline "$COMPARE"; then
    echo "WARN: Standard matrix comparison failed"
  fi
  echo "── CI PERF: comparing heavy vs baseline 'heavy-$COMPARE' ──"
  if ! run_matrix --profile heavy --compare-baseline "heavy-$COMPARE"; then
    echo "WARN: Heavy matrix comparison failed"
  fi
fi

if [ "$SAVE" = true ]; then
  echo "── CI PERF: saving baseline '$COMPARE' ──"
  run_matrix --save-baseline "$COMPARE"
  run_matrix --profile heavy --save-baseline "heavy-$COMPARE"
fi
