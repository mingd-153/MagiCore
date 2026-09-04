#!/bin/bash
# MagiCore Benchmark Runner — Reproducible PM Comparison
# Run: ./run_benchmark.sh <pm_name> <run_number>
# Example: ./run_benchmark.sh mgc 1

set -euo pipefail

# Get script directory for relative paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHMARK_ROOT="$(dirname "$SCRIPT_DIR")"

PM_NAME="${1:-mgc}"
RUN_NUM="${2:-1}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_DIR="$BENCHMARK_ROOT/results"
PACKAGE_JSON="${PACKAGE_JSON:-$BENCHMARK_ROOT/env/package.json}"

# Find mgc binary
MGC_BIN=$(which mgc 2>/dev/null || echo "/opt/homebrew/bin/mgc")

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
  "cpu": "$(if command -v lscpu &> /dev/null; then lscpu | grep 'Model name' | sed 's/Model name: *//' | xargs; else sysctl -n machdep.cpu.brand_string 2>/dev/null || echo 'Unknown CPU'; fi)",
  "cores": $(if command -v nproc &> /dev/null; then nproc; else sysctl -n hw.ncpu 2>/dev/null || echo 0; fi),
  "memory_gb": $(if command -v free &> /dev/null; then free -g | awk '/^Mem:/{print $2}'; else echo $(($(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1024 / 1024 / 1024)); fi),
  "os": "$(uname -s) $(uname -r)",
  "node_version": "$(node --version 2>/dev/null || echo 'N/A')",
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
START_MEMORY=$(if command -v free &> /dev/null; then free -m | awk '/^Mem:/{print $3}'; else echo 0; fi)

case "$PM_NAME" in
  mgc)
    if command -v gtime &> /dev/null; then
      gtime -v "$MGC_BIN" install > install.log 2>&1 || true
    else
      "$MGC_BIN" install > install.log 2>&1 || true
    fi
    ;;
  pnpm)
    if command -v gtime &> /dev/null; then
      gtime -v pnpm install > install.log 2>&1 || true
    else
      pnpm install > install.log 2>&1 || true
    fi
    ;;
  bun)
    if command -v gtime &> /dev/null; then
      gtime -v bun install > install.log 2>&1 || true
    else
      bun install > install.log 2>&1 || true
    fi
    ;;
  npm)
    if command -v gtime &> /dev/null; then
      gtime -v npm install > install.log 2>&1 || true
    else
      npm install > install.log 2>&1 || true
    fi
    ;;
  *)
    echo -e "${RED}Unknown PM: $PM_NAME${NC}"
    exit 1
    ;;
esac

END_TIME=$(date +%s.%N)
END_MEMORY=$(if command -v free &> /dev/null; then free -m | awk '/^Mem:/{print $3}'; else echo 0; fi)

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
    "$MGC_BIN" install > install_warm.log 2>&1 || true
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
