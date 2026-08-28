#!/bin/bash
# MagiCore Phased Benchmark Runner - Measures separate phases
# Run: ./run_benchmark_phased.sh <pm_name> <run_number>
# Example: ./run_benchmark_phased.sh mgc 1

set -euo pipefail

PM_NAME="${1:-mgc}"
RUN_NUM="${2:-1}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHMARK_ROOT="$(dirname "$SCRIPT_DIR")"
RESULTS_DIR="$BENCHMARK_ROOT/results/phased"
PACKAGE_JSON="$BENCHMARK_ROOT/env/package-unified.json"  # NEW: unified 20-package manifest
MGC_BINARY="$BENCHMARK_ROOT/../target/release/mgc"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${GREEN}=== MagiCore Phased Benchmark ===${NC}"
echo "PM: $PM_NAME | Run: $RUN_NUM | Time: $TIMESTAMP"
echo ""

# Timer helper
start_timer() {
  TIMER_START=$(gdate +%s.%N 2>/dev/null || date +%s)
}

stop_timer() {
  TIMER_END=$(gdate +%s.%N 2>/dev/null || date +%s)
  echo $(echo "$TIMER_END - $TIMER_START" | bc 2>/dev/null || echo "0")
}

# Check PM availability
echo -e "${YELLOW}[Check] PM availability...${NC}"
case "$PM_NAME" in
  mgc)
    if [ ! -f "$MGC_BINARY" ]; then
      echo -e "${RED}Error: mgc not found${NC}"
      exit 1
    fi
    PM_CMD="$MGC_BINARY install-web"
    ;;
  pnpm)
    if ! command -v pnpm &> /dev/null; then
      echo -e "${RED}Error: pnpm not installed${NC}"
      exit 1
    fi
    PM_CMD="pnpm install --ignore-scripts"
    ;;
  bun)
    if ! command -v bun &> /dev/null; then
      echo -e "${RED}Error: bun not installed${NC}"
      exit 1
    fi
    PM_CMD="bun install"
    ;;
  npm)
    if ! command -v npm &> /dev/null; then
      echo -e "${RED}Error: npm not installed${NC}"
      exit 1
    fi
    PM_CMD="npm install"
    ;;
  *)
    echo -e "${RED}Unknown PM: $PM_NAME${NC}"
    exit 1
    ;;
esac

# Machine spec
echo -e "${YELLOW}[Collect] Machine spec...${NC}"
CPU_MODEL=$(sysctl -n machdep.cpu.brand_string)
CPU_CORES=$(sysctl -n hw.ncpu)
MEMORY_GB=$(echo "$(sysctl -n hw.memsize) / 1024 / 1024 / 1024" | bc)
OS_VERSION="$(sw_vers -productName) $(sw_vers -productVersion)"
NODE_VERSION=$(node --version)

echo "  CPU: $CPU_MODEL ($CPU_CORES cores)"
echo "  RAM: ${MEMORY_GB}GB"
echo "  OS: $OS_VERSION"
echo "  Node: $NODE_VERSION"

# Setup workspace
WORK_DIR="/tmp/benchmark_phased_${PM_NAME}_${RUN_NUM}_${TIMESTAMP}"
mkdir -p "$WORK_DIR"
cp "$PACKAGE_JSON" "$WORK_DIR/package.json"
cd "$WORK_DIR"
echo -e "${BLUE}Workspace: $WORK_DIR${NC}"

# Package count
PKG_COUNT=$(cat package.json | jq '[.dependencies, .devDependencies] | add | length')
echo "  Packages: $PKG_COUNT direct"

# === PHASE 1: COLD INSTALL (clean cache) ===
echo ""
echo -e "${YELLOW}=== PHASE 1: COLD INSTALL ===${NC}"

# Clean cache
echo -e "${BLUE}[1.1] Cleaning cache...${NC}"
case "$PM_NAME" in
  mgc)
    rm -rf ~/.magicore/store ~/.magicore/cache || true
    ;;
  pnpm)
    pnpm store prune 2>/dev/null || true
    ;;
  bun)
    rm -rf ~/.bun/install/cache || true
    ;;
  npm)
    npm cache clean --force 2>/dev/null || true
    ;;
esac

sleep 1

# Run cold install
echo -e "${BLUE}[1.2] Running cold install...${NC}"
start_timer
$PM_CMD > install_cold.log 2>&1
COLD_DURATION=$(stop_timer)
COLD_DISK=$(du -sm node_modules 2>/dev/null | cut -f1 || echo "0")

echo -e "${GREEN}  ✓ Cold install: ${COLD_DURATION}s (${COLD_DISK}MB)${NC}"

# === PHASE 2: WARM INSTALL (cache hit) ===
echo ""
echo -e "${YELLOW}=== PHASE 2: WARM INSTALL ===${NC}"

rm -rf node_modules
sleep 1

echo -e "${BLUE}[2.1] Running warm install (cached)...${NC}"
start_timer
$PM_CMD > install_warm.log 2>&1
WARM_DURATION=$(stop_timer)
WARM_DISK=$(du -sm node_modules 2>/dev/null | cut -f1 || echo "0")

echo -e "${GREEN}  ✓ Warm install: ${WARM_DURATION}s (${WARM_DISK}MB)${NC}"

# === PHASE 3: OFFLINE INSTALL (no network) ===
echo ""
echo -e "${YELLOW}=== PHASE 3: OFFLINE INSTALL ===${NC}"

rm -rf node_modules
sleep 1

echo -e "${BLUE}[3.1] Running offline install (cached, no registry)...${NC}"

# Disable network (best effort - depends on PM support)
start_timer
case "$PM_NAME" in
  mgc)
    # mgc doesn't have offline flag yet - same as warm
    $PM_CMD > install_offline.log 2>&1
    ;;
  pnpm)
    pnpm install --offline --ignore-scripts > install_offline.log 2>&1
    ;;
  bun)
    # bun doesn't have offline flag - same as warm
    bun install > install_offline.log 2>&1
    ;;
  npm)
    npm install --offline > install_offline.log 2>&1
    ;;
esac
OFFLINE_DURATION=$(stop_timer)

echo -e "${GREEN}  ✓ Offline install: ${OFFLINE_DURATION}s${NC}"

# === PHASE 4: INCREMENTAL (add one package) ===
echo ""
echo -e "${YELLOW}=== PHASE 4: INCREMENTAL ADD ===${NC}"

echo -e "${BLUE}[4.1] Adding 'ms' package...${NC}"
start_timer
case "$PM_NAME" in
  mgc)
    "$MGC_BINARY" add ms > add.log 2>&1
    ;;
  pnpm)
    pnpm add ms --ignore-scripts > add.log 2>&1
    ;;
  bun)
    bun add ms > add.log 2>&1
    ;;
  npm)
    npm install ms > add.log 2>&1
    ;;
esac
INCREMENTAL_DURATION=$(stop_timer)

echo -e "${GREEN}  ✓ Incremental add: ${INCREMENTAL_DURATION}s${NC}"

# Calculate speedup
WARM_SPEEDUP=$(echo "scale=1; (($COLD_DURATION - $WARM_DURATION) / $COLD_DURATION) * 100" | bc)
OFFLINE_SPEEDUP=$(echo "scale=1; (($COLD_DURATION - $OFFLINE_DURATION) / $COLD_DURATION) * 100" | bc)

echo ""
echo -e "${GREEN}=== Summary ===${NC}"
echo "  Cold:        ${COLD_DURATION}s (${COLD_DISK}MB)"
echo "  Warm:        ${WARM_DURATION}s (${WARM_SPEEDUP}% faster)"
echo "  Offline:     ${OFFLINE_DURATION}s (${OFFLINE_SPEEDUP}% faster)"
echo "  Incremental: ${INCREMENTAL_DURATION}s"

# Save results
mkdir -p "$RESULTS_DIR"
RESULT_FILE="$RESULTS_DIR/${PM_NAME}_run${RUN_NUM}_${TIMESTAMP}.json"

cat > "$RESULT_FILE" <<EOF
{
  "pm": "$PM_NAME",
  "run": $RUN_NUM,
  "timestamp": "$TIMESTAMP",
  "machine": {
    "cpu": "$CPU_MODEL",
    "cores": $CPU_CORES,
    "memory_gb": $MEMORY_GB,
    "os": "$OS_VERSION",
    "node_version": "$NODE_VERSION"
  },
  "package_count": $PKG_COUNT,
  "phases": {
    "cold": {
      "duration_seconds": $COLD_DURATION,
      "disk_mb": $COLD_DISK,
      "notes": "Clean cache, full download + install"
    },
    "warm": {
      "duration_seconds": $WARM_DURATION,
      "disk_mb": $WARM_DISK,
      "speedup_percent": $WARM_SPEEDUP,
      "notes": "Cache hit, no download"
    },
    "offline": {
      "duration_seconds": $OFFLINE_DURATION,
      "speedup_percent": $OFFLINE_SPEEDUP,
      "notes": "No network, cache only"
    },
    "incremental": {
      "duration_seconds": $INCREMENTAL_DURATION,
      "notes": "Add single package to existing install"
    }
  },
  "manifest": "package-unified.json (20 packages, no vitest)"
}
EOF

echo ""
echo -e "${GREEN}✓ Results saved: $RESULT_FILE${NC}"
cat "$RESULT_FILE" | jq .

# Cleanup
cd /tmp
rm -rf "$WORK_DIR"

echo -e "${GREEN}=== Phased Benchmark Complete ===${NC}"
