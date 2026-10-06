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
PACKAGE_JSON="$BENCHMARK_ROOT/env/package-unified.json"
PROJECT_ROOT="$(cd "$BENCHMARK_ROOT/.." && pwd)"
MGC_BINARY="${MGC_BIN:-$PROJECT_ROOT/target/release/mgc}"
if [ ! -x "$MGC_BINARY" ] && [ -x "$PROJECT_ROOT/target/debug/mgc" ]; then
  MGC_BINARY="$PROJECT_ROOT/target/debug/mgc"
fi
if [ ! -x "$MGC_BINARY" ] && command -v mgc >/dev/null 2>&1; then
  MGC_BINARY="$(command -v mgc)"
fi
WORK_DIR=""
PM_HOME="$(mktemp -d "${TMPDIR:-/tmp}/mgc-phased-home.XXXXXX")"
export HOME="$PM_HOME"
export XDG_CACHE_HOME="$PM_HOME/.cache"
export MGC_CACHE_DIR="$PM_HOME/.mgc"
export npm_config_cache="$PM_HOME/npm-cache"
export YARN_CACHE_FOLDER="$PM_HOME/yarn-cache"
export BUN_INSTALL_CACHE_DIR="$PM_HOME/bun-cache"
mkdir -p "$XDG_CACHE_HOME" "$MGC_CACHE_DIR" "$PM_HOME/pnpm-store"
cleanup() {
  [ -n "$WORK_DIR" ] && rm -rf "$WORK_DIR"
  rm -rf "$PM_HOME"
}
trap cleanup EXIT

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
now_seconds() {
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import time; print(f"{time.time_ns() / 1_000_000_000:.9f}")'
  elif command -v gdate >/dev/null 2>&1; then
    gdate +%s.%N
  else
    date +%s
  fi
}

start_timer() {
  TIMER_START=$(now_seconds)
}

stop_timer() {
  TIMER_END=$(now_seconds)
  echo "$(echo "$TIMER_END - $TIMER_START" | bc)"
}

# Check PM availability
echo -e "${YELLOW}[Check] PM availability...${NC}"
case "$PM_NAME" in
  mgc)
    if [ ! -x "$MGC_BINARY" ]; then
      echo -e "${RED}Error: mgc executable not found; set MGC_BIN${NC}"
      exit 1
    fi
    ;;
  pnpm)
    if ! command -v pnpm &> /dev/null; then
      echo -e "${RED}Error: pnpm not installed${NC}"
      exit 1
    fi
    ;;
  bun)
    if ! command -v bun &> /dev/null; then
      echo -e "${RED}Error: bun not installed${NC}"
      exit 1
    fi
    ;;
  npm)
    if ! command -v npm &> /dev/null; then
      echo -e "${RED}Error: npm not installed${NC}"
      exit 1
    fi
    ;;
  *)
    echo -e "${RED}Unknown PM: $PM_NAME${NC}"
    exit 1
    ;;
esac

run_pm_install() {
  case "$PM_NAME" in
    mgc) "$MGC_BINARY" install-web ;;
    pnpm) pnpm install --ignore-scripts --store-dir "$PM_HOME/pnpm-store" ;;
    bun) bun install --ignore-scripts ;;
    npm) npm install --ignore-scripts ;;
  esac
}

# Machine spec
echo -e "${YELLOW}[Collect] Machine spec...${NC}"
CPU_MODEL="$(sysctl -n machdep.cpu.brand_string 2>/dev/null || (command -v lscpu >/dev/null 2>&1 && lscpu | sed -n 's/^Model name:[[:space:]]*//p' | head -n 1) || uname -m)"
CPU_CORES="$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 0)"
# Try the Linux memory utility when present; macOS falls back to sysctl.
# (Dùng công cụ Linux khi có; macOS sẽ lấy dung lượng qua sysctl.)
MEMORY_GB=""
if command -v free >/dev/null 2>&1; then
  MEMORY_GB="$(free -g 2>/dev/null | awk '/^Mem:/ {print $2; exit}' || true)"
fi
if [ -z "$MEMORY_GB" ]; then
  MEMORY_BYTES="$(sysctl -n hw.memsize 2>/dev/null || echo 0)"
  MEMORY_GB="$(awk -v bytes="$MEMORY_BYTES" 'BEGIN { printf "%.0f", bytes / 1073741824 }')"
fi
OS_VERSION="$(uname -s) $(uname -r)"
NODE_VERSION="$(node --version 2>/dev/null || echo N/A)"

echo "  CPU: $CPU_MODEL ($CPU_CORES cores)"
echo "  RAM: ${MEMORY_GB}GB"
echo "  OS: $OS_VERSION"
echo "  Node: $NODE_VERSION"

# Setup workspace
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/benchmark_phased_${PM_NAME}_${RUN_NUM}_${TIMESTAMP}.XXXXXX")"
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

echo -e "${BLUE}[1.1] Using isolated HOME/store for a cold cache...${NC}"

sleep 1

# Run cold install
echo -e "${BLUE}[1.2] Running cold install...${NC}"
start_timer
run_pm_install > install_cold.log 2>&1
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
run_pm_install > install_warm.log 2>&1
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
OFFLINE_DURATION=""
start_timer
case "$PM_NAME" in
  mgc)
    # No explicit offline option exists yet; don't label a warm retry offline.
    # (Chưa có cờ offline riêng; không giả nhãn lần chạy cache-hit là offline.)
    echo "  SKIP: mgc has no explicit offline-install option"
    OFFLINE_DURATION="null"
    ;;
  pnpm)
    pnpm install --offline --ignore-scripts > install_offline.log 2>&1
    ;;
  bun)
    echo "  SKIP: Bun has no explicit offline-install option"
    OFFLINE_DURATION="null"
    ;;
  npm)
    npm install --offline > install_offline.log 2>&1
    ;;
esac
if [ "$OFFLINE_DURATION" != "null" ]; then
  OFFLINE_DURATION=$(stop_timer)
fi

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
if [ "$OFFLINE_DURATION" = "null" ]; then
  OFFLINE_SPEEDUP="null"
else
  OFFLINE_SPEEDUP=$(echo "scale=1; (($COLD_DURATION - $OFFLINE_DURATION) / $COLD_DURATION) * 100" | bc)
fi

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
