#!/bin/bash
# Quick benchmark - cold and warm only
set -euo pipefail

PM="${1:-mgc}"
RUN="${2:-1}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${PROJECT_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
TS=$(date +%Y%m%d_%H%M%S)

MGC="${MGC_BIN:-$ROOT/target/release/mgc}"
if [ ! -x "$MGC" ]; then
  MGC="$(command -v mgc || true)"
fi
if [ "$PM" = mgc ] && [ -z "$MGC" ]; then
  echo "mgc binary not found; set MGC_BIN or build target/release/mgc" >&2
  exit 1
fi
case "$PM" in mgc|pnpm) ;; *) echo "unsupported benchmark tool: $PM" >&2; exit 2 ;; esac
PKG="$ROOT/benchmark/env/package-unified.json"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/qbench_${PM}_${RUN}_${TS}.XXXXXX")"
PM_HOME="$(mktemp -d "${TMPDIR:-/tmp}/qbench-home_${PM}_${RUN}.XXXXXX")"
RESULTS="$ROOT/benchmark/results/phased"
trap 'rm -rf "$WORK" "$PM_HOME"' EXIT

mkdir -p "$WORK" "$RESULTS"
cp "$PKG" "$WORK/package.json"
cd "$WORK"

echo "=== Quick Bench: $PM run $RUN ==="
echo "Workspace: $WORK"

mkdir -p "$PM_HOME/.cache" "$PM_HOME/.mgc" "$PM_HOME/pnpm-store"

# Cold
echo "[COLD]"
START=$(python3 -c 'import time; print(f"{time.time_ns() / 1_000_000_000:.9f}")')
case "$PM" in
  mgc) HOME="$PM_HOME" XDG_CACHE_HOME="$PM_HOME/.cache" MGC_CACHE_DIR="$PM_HOME/.mgc" "$MGC" install-web > cold.log 2>&1 ;;
  pnpm) HOME="$PM_HOME" XDG_CACHE_HOME="$PM_HOME/.cache" pnpm install --ignore-scripts --store-dir "$PM_HOME/pnpm-store" > cold.log 2>&1 ;;
esac
END=$(python3 -c 'import time; print(f"{time.time_ns() / 1_000_000_000:.9f}")')
COLD=$(echo "$END - $START" | bc)
DISK=$(du -sm node_modules | cut -f1)
echo "  Time: ${COLD}s"
echo "  Disk: ${DISK}MB"

# Warm
echo "[WARM]"
rm -rf node_modules
sleep 1
START=$(python3 -c 'import time; print(f"{time.time_ns() / 1_000_000_000:.9f}")')
case "$PM" in
  mgc) HOME="$PM_HOME" XDG_CACHE_HOME="$PM_HOME/.cache" MGC_CACHE_DIR="$PM_HOME/.mgc" "$MGC" install-web > warm.log 2>&1 ;;
  pnpm) HOME="$PM_HOME" XDG_CACHE_HOME="$PM_HOME/.cache" pnpm install --ignore-scripts --store-dir "$PM_HOME/pnpm-store" > warm.log 2>&1 ;;
esac
END=$(python3 -c 'import time; print(f"{time.time_ns() / 1_000_000_000:.9f}")')
WARM=$(echo "$END - $START" | bc)
echo "  Time: ${WARM}s"

# Save
SPEEDUP=$(echo "scale=1; (($COLD - $WARM) / $COLD) * 100" | bc)
cat > "$RESULTS/${PM}_run${RUN}_${TS}.json" <<EOF
{
  "pm": "$PM",
  "run": $RUN,
  "timestamp": "$TS",
  "machine": {
    "cpu": "$(sysctl -n machdep.cpu.brand_string 2>/dev/null || (command -v lscpu >/dev/null 2>&1 && lscpu | sed -n 's/^Model name:[[:space:]]*//p' | head -n 1) || uname -m)",
    "cores": $(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 0),
    "os": "$(uname -s) $(uname -r)"
  },
  "cold": {
    "seconds": $COLD,
    "disk_mb": $DISK
  },
  "warm": {
    "seconds": $WARM,
    "speedup_pct": $SPEEDUP
  }
}
EOF

echo ""
echo "✓ Results: $RESULTS/${PM}_run${RUN}_${TS}.json"
cat "$RESULTS/${PM}_run${RUN}_${TS}.json"
