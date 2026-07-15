#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAB_DIR="$ROOT/tools/core-web-lab"
STAMP="$(date '+%Y%m%d-%H%M%S')"
MODE="${1:-quick}"

mkdir -p "$LAB_DIR/benchmarks"

case "$MODE" in
  quick)
    "$ROOT/scripts/bench.sh" matrix | tee "$LAB_DIR/benchmarks/matrix-$STAMP.log"
    ;;
  heavy)
    "$ROOT/scripts/bench.sh" matrix-heavy | tee "$LAB_DIR/benchmarks/matrix-heavy-$STAMP.log"
    ;;
  full)
    "$ROOT/scripts/bench.sh" matrix | tee "$LAB_DIR/benchmarks/matrix-$STAMP.log"
    "$ROOT/scripts/bench.sh" matrix-heavy | tee "$LAB_DIR/benchmarks/matrix-heavy-$STAMP.log"
    ;;
  *)
    echo "usage: $0 [quick|heavy|full]" >&2
    exit 1
    ;;
esac

echo "benchmark lane complete: $MODE"
