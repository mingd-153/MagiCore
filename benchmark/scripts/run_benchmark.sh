#!/bin/bash
# MagiCore Benchmark Runner — Reproducible PM Comparison
# Run: ./run_benchmark.sh <pm_name> <run_number>
# Example: ./run_benchmark.sh mgc 1

set -euo pipefail

PM_NAME="${1:-mgc}"
RUN_NUM="${2:-1}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_DIR="/benchmark/results"
PACKAGE_JSON="/benchmark/env/package.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== MagiCore Benchmark Runner ===${NC}"
echo "PM: $PM_NAME | Run: $RUN_NUM | Time: $TIMESTAMP"
echo ""

# Machine spec
echo -e "${YELLOW}[1/6] Collecting machine spec...${NC}"
MACHINE_SPEC=$(cat <<EOF
{
  "cpu": "$(lscpu | grep 'Model name' | sed 's/Model name: *//' | xargs)",
  "cores": $(nproc),
  "memory_gb": $(free -g | awk '/^Mem:/{print $2}'),
  "os": "$(uname -s) $(uname -r)",
  "node_version": "$(node --version)",
  "timestamp": "$TIMESTAMP"
}
EOF
)
echo "$MACHINE_SPEC" | jq .

# Setup clean workspace
echo -e "${YELLOW}[2/6] Setting up clean workspace...${NC}"
WORK_DIR="/tmp/benchmark_${PM_NAME}_${RUN_NUM}_${TIMESTAMP}"
mkdir -p "$WORK_DIR"
cp "$PACKAGE_JSON" "$WORK_DIR/package.json"
cd "$WORK_DIR"

# Clean caches
echo -e "${YELLOW}[3/6] Cleaning PM caches...${NC}"
case "$PM_NAME" in
  mgc)
    rm -rf ~/.magicore/store ~/.magicore/cache || true
    ;;
  pnpm)
    pnpm store prune || true
    ;;
  bun)
    rm -rf ~/.bun/install/cache || true
    ;;
  npm)
    npm cache clean --force || true
    ;;
esac

# Pre-benchmark sync
sync
sleep 2

# Run benchmark (cold install)
echo -e "${YELLOW}[4/6] Running COLD install with $PM_NAME...${NC}"
START_TIME=$(date +%s.%N)
START_MEMORY=$(free -m | awk '/^Mem:/{print $3}')

case "$PM_NAME" in
  mgc)
    /usr/bin/time -v /benchmark/mgc install > install.log 2>&1 || true
    ;;
  pnpm)
    /usr/bin/time -v pnpm install > install.log 2>&1 || true
    ;;
  bun)
    /usr/bin/time -v bun install > install.log 2>&1 || true
    ;;
  npm)
    /usr/bin/time -v npm install > install.log 2>&1 || true
    ;;
  *)
    echo -e "${RED}Unknown PM: $PM_NAME${NC}"
    exit 1
    ;;
esac

END_TIME=$(date +%s.%N)
END_MEMORY=$(free -m | awk '/^Mem:/{print $3}')

# Calculate metrics
DURATION=$(echo "$END_TIME - $START_TIME" | bc)
DISK_USAGE=$(du -sm node_modules 2>/dev/null | cut -f1 || echo "0")
MEMORY_USED=$(echo "$END_MEMORY - $START_MEMORY" | bc)

echo -e "${GREEN}✓ Install complete${NC}"
echo "  Duration: ${DURATION}s"
echo "  Disk: ${DISK_USAGE}MB"
echo "  Memory delta: ${MEMORY_USED}MB"

# Warm install (cached)
echo -e "${YELLOW}[5/6] Running WARM install with $PM_NAME...${NC}"
rm -rf node_modules
sync
sleep 1

WARM_START=$(date +%s.%N)
case "$PM_NAME" in
  mgc)
    /benchmark/mgc install > install_warm.log 2>&1 || true
    ;;
  pnpm)
    pnpm install > install_warm.log 2>&1 || true
    ;;
  bun)
    bun install > install_warm.log 2>&1 || true
    ;;
  npm)
    npm install > install_warm.log 2>&1 || true
    ;;
esac
WARM_END=$(date +%s.%N)
WARM_DURATION=$(echo "$WARM_END - $WARM_START" | bc)

echo -e "${GREEN}✓ Warm install complete: ${WARM_DURATION}s${NC}"

# Generate JSON result
echo -e "${YELLOW}[6/6] Generating result JSON...${NC}"
RESULT_FILE="$RESULTS_DIR/${PM_NAME}_run${RUN_NUM}_${TIMESTAMP}.json"
cat > "$RESULT_FILE" <<EOF
{
  "pm": "$PM_NAME",
  "run": $RUN_NUM,
  "timestamp": "$TIMESTAMP",
  "machine": $MACHINE_SPEC,
  "cold_install": {
    "duration_seconds": $DURATION,
    "disk_mb": $DISK_USAGE,
    "memory_delta_mb": $MEMORY_USED
  },
  "warm_install": {
    "duration_seconds": $WARM_DURATION
  },
  "package_count": $(cat package.json | jq '[.dependencies, .devDependencies] | add | length'),
  "logs": {
    "cold": "$(cat install.log | head -20 | sed 's/"/\\"/g' | tr '\n' ' ')",
    "warm": "$(cat install_warm.log | head -20 | sed 's/"/\\"/g' | tr '\n' ' ')"
  }
}
EOF

echo -e "${GREEN}✓ Result saved: $RESULT_FILE${NC}"
cat "$RESULT_FILE" | jq .

# Cleanup
rm -rf "$WORK_DIR"

echo -e "${GREEN}=== Benchmark Complete ===${NC}"
