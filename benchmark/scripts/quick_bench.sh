#!/bin/bash
# Quick benchmark - cold and warm only
set -euo pipefail

PM="${1:-mgc}"
RUN="${2:-1}"
TS=$(date +%Y%m%d_%H%M%S)

ROOT="/Users/doanmihh/Documents/Workspace/MagiCore"
MGC="$ROOT/target/release/mgc"
PKG="$ROOT/benchmark/env/package-unified.json"
WORK="/tmp/qbench_${PM}_${RUN}_${TS}"
RESULTS="$ROOT/benchmark/results/phased"

mkdir -p "$WORK" "$RESULTS"
cp "$PKG" "$WORK/package.json"
cd "$WORK"

echo "=== Quick Bench: $PM run $RUN ==="
echo "Workspace: $WORK"

# Clean cache
case "$PM" in
  mgc) rm -rf ~/.magicore/store ~/.magicore/cache ;;
  pnpm) pnpm store prune 2>/dev/null || true ;;
esac

# Cold
echo "[COLD]"
START=$(gdate +%s.%N)
case "$PM" in
  mgc) $MGC install-web > cold.log 2>&1 ;;
  pnpm) pnpm install --ignore-scripts > cold.log 2>&1 ;;
esac
END=$(gdate +%s.%N)
COLD=$(echo "$END - $START" | bc)
DISK=$(du -sm node_modules | cut -f1)
echo "  Time: ${COLD}s"
echo "  Disk: ${DISK}MB"

# Warm
echo "[WARM]"
rm -rf node_modules
sleep 1
START=$(gdate +%s.%N)
case "$PM" in
  mgc) $MGC install-web > warm.log 2>&1 ;;
  pnpm) pnpm install --ignore-scripts > warm.log 2>&1 ;;
esac
END=$(gdate +%s.%N)
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
    "cpu": "$(sysctl -n machdep.cpu.brand_string)",
    "cores": $(sysctl -n hw.ncpu),
    "os": "$(sw_vers -productName) $(sw_vers -productVersion)"
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

cd /tmp && rm -rf "$WORK"
