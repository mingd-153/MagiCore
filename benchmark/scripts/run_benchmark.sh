#!/bin/bash
# MagiCore Benchmark Runner — Reproducible PM Comparison
# Run: ./run_benchmark.sh <pm_name> <run_number>
# Example: ./run_benchmark.sh mgc 1

set -euo pipefail

# Get script directory for relative paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHMARK_ROOT="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(cd "$BENCHMARK_ROOT/.." && pwd)"

PM_NAME="${1:-mgc}"
RUN_NUM="${2:-1}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_DIR="$BENCHMARK_ROOT/results"
PACKAGE_JSON="${PACKAGE_JSON:-$BENCHMARK_ROOT/env/package.json}"

# Prefer this checkout's binary; callers can override it for a chosen build.
# (Ưu tiên binary trong checkout; có thể override để chọn bản build.)
MGC_BIN="${MGC_BIN:-}"
if [ -z "$MGC_BIN" ] && [ -x "$PROJECT_ROOT/target/release/mgc" ]; then
  MGC_BIN="$PROJECT_ROOT/target/release/mgc"
elif [ -z "$MGC_BIN" ] && [ -x "$PROJECT_ROOT/target/debug/mgc" ]; then
  MGC_BIN="$PROJECT_ROOT/target/debug/mgc"
elif [ -z "$MGC_BIN" ]; then
  MGC_BIN="$(command -v mgc || true)"
fi
case "$PM_NAME" in
  mgc) [ -x "$MGC_BIN" ] || { echo "mgc binary not found; set MGC_BIN" >&2; exit 1; } ;;
  pnpm|bun|npm|yarn) command -v "$PM_NAME" >/dev/null 2>&1 || { echo "$PM_NAME is not installed" >&2; exit 1; } ;;
  *) echo "Unknown PM: $PM_NAME" >&2; exit 2 ;;
esac
command -v jq >/dev/null 2>&1 || { echo "jq is required to record benchmark JSON" >&2; exit 1; }

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== MagiCore Benchmark Runner ===${NC}"
echo "PM: $PM_NAME | Run: $RUN_NUM | Time: $TIMESTAMP"
echo ""

# Machine spec — portable across macOS and Linux.
echo -e "${YELLOW}[1/6] Collecting machine spec...${NC}"
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
MACHINE_SPEC=$(jq -n --arg cpu "$CPU_MODEL" --arg os "$(uname -s) $(uname -r)" \
  --arg node "$(node --version 2>/dev/null || echo N/A)" --arg timestamp "$TIMESTAMP" \
  --argjson cores "$CPU_CORES" --argjson memory "$MEMORY_GB" \
  '{cpu:$cpu,cores:$cores,memory_gb:$memory,os:$os,node_version:$node,timestamp:$timestamp}')
echo "$MACHINE_SPEC" | jq .

# Use disposable workspace and HOME; do not prune the user's package caches.
# (Dùng workspace và HOME tạm; không dọn cache của người dùng.)
echo -e "${YELLOW}[2/6] Setting up clean workspace...${NC}"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/benchmark_${PM_NAME}_${RUN_NUM}_${TIMESTAMP}.XXXXXX")"
PM_HOME="$(mktemp -d "${TMPDIR:-/tmp}/benchmark-home_${PM_NAME}_${RUN_NUM}.XXXXXX")"
mkdir -p "$RESULTS_DIR" "$PM_HOME/.cache" "$PM_HOME/.mgc" "$PM_HOME/pnpm-store"
export HOME="$PM_HOME"
export XDG_CACHE_HOME="$PM_HOME/.cache"
export MGC_CACHE_DIR="$PM_HOME/.mgc"
export npm_config_cache="$PM_HOME/npm-cache"
export YARN_CACHE_FOLDER="$PM_HOME/yarn-cache"
export BUN_INSTALL_CACHE_DIR="$PM_HOME/bun-cache"
ACTIVE_INSTALL_LOG=""
cleanup() {
  local status=$?
  if [ "$status" -ne 0 ] && [ -n "$ACTIVE_INSTALL_LOG" ] && [ -f "$ACTIVE_INSTALL_LOG" ]; then
    echo "Install failed; last log lines:" >&2
    tail -n 30 "$ACTIVE_INSTALL_LOG" >&2
  fi
  rm -rf "$WORK_DIR" "$PM_HOME"
}
trap cleanup EXIT
cp "$PACKAGE_JSON" "$WORK_DIR/package.json"
cd "$WORK_DIR"

echo -e "${YELLOW}[3/6] Preparing isolated package-manager cache...${NC}"
run_install() {
  case "$PM_NAME" in
    mgc) "$MGC_BIN" install ;;
    pnpm) pnpm install --ignore-scripts --store-dir "$PM_HOME/pnpm-store" ;;
    bun) bun install --ignore-scripts ;;
    npm) npm install --ignore-scripts ;;
    # This benchmark measures install only; match other PMs' warning behavior
    # for Node engine ranges because no package runtime is executed.
    # (Chỉ đo install; bỏ engine gate riêng của Yarn vì không chạy package.)
    yarn) yarn install --ignore-scripts --ignore-engines --non-interactive ;;
  esac
}
now_seconds() {
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import time; print(f"{time.time_ns() / 1_000_000_000:.9f}")'
  elif command -v gdate >/dev/null 2>&1; then
    gdate +%s.%N
  else
    date +%s
  fi
}

# Pre-benchmark sync
sync
sleep 2

# Run benchmark (cold install)
echo -e "${YELLOW}[4/6] Running COLD install with $PM_NAME...${NC}"
START_TIME=$(now_seconds)
START_MEMORY=$(if command -v free &> /dev/null; then free -m | awk '/^Mem:/{print $3}'; else echo 0; fi)
ACTIVE_INSTALL_LOG="$WORK_DIR/install.log"
run_install > "$ACTIVE_INSTALL_LOG" 2>&1

END_TIME=$(now_seconds)
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

WARM_START=$(now_seconds)
ACTIVE_INSTALL_LOG="$WORK_DIR/install_warm.log"
run_install > "$ACTIVE_INSTALL_LOG" 2>&1
WARM_END=$(now_seconds)
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

echo -e "${GREEN}=== Benchmark Complete ===${NC}"
