#!/bin/bash
# MagiCore Benchmark Runner — Native macOS Version
# Run: ./run_benchmark_native.sh <pm_name> <run_number>
# Example: ./run_benchmark_native.sh mgc 1

set -euo pipefail

PM_NAME="${1:-mgc}"
RUN_NUM="${2:-1}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHMARK_ROOT="$(dirname "$SCRIPT_DIR")"
RESULTS_DIR="$BENCHMARK_ROOT/results"
PACKAGE_JSON="$BENCHMARK_ROOT/env/package.json"
PACKAGE_JSON_SIMPLE="$BENCHMARK_ROOT/env/package-simple.json"
MGC_BINARY="$BENCHMARK_ROOT/../target/release/mgc"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}=== MagiCore Benchmark (Native macOS) ===${NC}"
echo "PM: $PM_NAME | Run: $RUN_NUM | Time: $TIMESTAMP"
echo ""

# Check PM availability
echo -e "${YELLOW}[1/6] Checking PM availability...${NC}"
case "$PM_NAME" in
  mgc)
    if [ ! -f "$MGC_BINARY" ]; then
      echo -e "${RED}Error: mgc binary not found at $MGC_BINARY${NC}"
      echo "Run: cargo build --release"
      exit 1
    fi
    echo "✓ mgc: $MGC_BINARY"
    ;;
  pnpm)
    if ! command -v pnpm &> /dev/null; then
      echo -e "${RED}Error: pnpm not installed${NC}"
      echo "Run: npm install -g pnpm"
      exit 1
    fi
    echo "✓ pnpm: $(command -v pnpm)"
    ;;
  bun)
    if ! command -v bun &> /dev/null; then
      echo -e "${RED}Error: bun not installed${NC}"
      echo "Run: curl -fsSL https://bun.sh/install | bash"
      exit 1
    fi
    echo "✓ bun: $(command -v bun)"
    ;;
  npm)
    if ! command -v npm &> /dev/null; then
      echo -e "${RED}Error: npm not installed${NC}"
      exit 1
    fi
    echo "✓ npm: $(command -v npm)"
    ;;
  *)
    echo -e "${RED}Unknown PM: $PM_NAME${NC}"
    exit 1
    ;;
esac

# Machine spec
echo -e "${YELLOW}[2/6] Collecting machine spec...${NC}"
MACHINE_SPEC=$(cat <<EOF
{
  "cpu": "$(sysctl -n machdep.cpu.brand_string)",
  "cores": $(sysctl -n hw.ncpu),
  "memory_gb": $(echo "$(sysctl -n hw.memsize) / 1024 / 1024 / 1024" | bc),
  "os": "$(uname -s) $(uname -r)",
  "node_version": "$(node --version)",
  "timestamp": "$TIMESTAMP"
}
EOF
)
echo "$MACHINE_SPEC" | jq .

# Setup clean workspace
echo -e "${YELLOW}[3/6] Setting up clean workspace...${NC}"
WORK_DIR="/tmp/benchmark_${PM_NAME}_${RUN_NUM}_${TIMESTAMP}"
mkdir -p "$WORK_DIR"

# mgc uses simple package (no Next.js), others use full package
if [ "$PM_NAME" = "mgc" ]; then
    cp "$PACKAGE_JSON_SIMPLE" "$WORK_DIR/package.json"
    echo "Using simple package.json for mgc (11 deps, no Next.js)"
else
    cp "$PACKAGE_JSON" "$WORK_DIR/package.json"
fi

cd "$WORK_DIR"
echo "Workspace: $WORK_DIR"

# Clean caches
echo -e "${YELLOW}[4/6] Cleaning PM caches...${NC}"
case "$PM_NAME" in
  mgc)
    rm -rf ~/.magicore/store ~/.magicore/cache || true
    echo "✓ Cleaned mgc cache"
    ;;
  pnpm)
    pnpm store prune || true
    echo "✓ Cleaned pnpm store"
    ;;
  bun)
    rm -rf ~/.bun/install/cache || true
    echo "✓ Cleaned bun cache"
    ;;
  npm)
    npm cache clean --force || true
    echo "✓ Cleaned npm cache"
    ;;
esac

sleep 1

# Run COLD install
echo -e "${YELLOW}[5/6] Running COLD install with $PM_NAME...${NC}"
START_TIME=$(gdate +%s.%N 2>/dev/null || date +%s)

case "$PM_NAME" in
  mgc)
    "$MGC_BINARY" install-web > install.log 2>&1
    ;;
  pnpm)
    pnpm install --ignore-scripts > install.log 2>&1
    ;;
  bun)
    bun install > install.log 2>&1
    ;;
  npm)
    npm install > install.log 2>&1
    ;;
esac

END_TIME=$(gdate +%s.%N 2>/dev/null || date +%s)
DURATION=$(echo "$END_TIME - $START_TIME" | bc 2>/dev/null || echo "N/A")
DISK_USAGE=$(du -sm node_modules 2>/dev/null | cut -f1 || echo "0")

echo -e "${GREEN}✓ Cold install complete${NC}"
echo "  Duration: ${DURATION}s"
echo "  Disk: ${DISK_USAGE}MB"

# Run WARM install
echo -e "${YELLOW}[6/6] Running WARM install with $PM_NAME...${NC}"
rm -rf node_modules
sleep 1

WARM_START=$(gdate +%s.%N 2>/dev/null || date +%s)
case "$PM_NAME" in
  mgc)
    "$MGC_BINARY" install-web > install_warm.log 2>&1
    ;;
  pnpm)
    pnpm install --ignore-scripts > install_warm.log 2>&1
    ;;
  bun)
    bun install > install_warm.log 2>&1
    ;;
  npm)
    npm install > install_warm.log 2>&1
    ;;
esac
WARM_END=$(gdate +%s.%N 2>/dev/null || date +%s)
WARM_DURATION=$(echo "$WARM_END - $WARM_START" | bc 2>/dev/null || echo "N/A")

echo -e "${GREEN}✓ Warm install complete: ${WARM_DURATION}s${NC}"

# Generate result
mkdir -p "$RESULTS_DIR"
RESULT_FILE="$RESULTS_DIR/${PM_NAME}_run${RUN_NUM}_${TIMESTAMP}.json"

cat > "$RESULT_FILE" <<EOF
{
  "pm": "$PM_NAME",
  "run": $RUN_NUM,
  "timestamp": "$TIMESTAMP",
  "machine": $MACHINE_SPEC,
  "cold_install": {
    "duration_seconds": "$DURATION",
    "disk_mb": $DISK_USAGE
  },
  "warm_install": {
    "duration_seconds": "$WARM_DURATION"
  },
  "package_count": $(cat package.json | jq '[.dependencies, .devDependencies] | add | length')
}
EOF

echo -e "${GREEN}✓ Result saved: $RESULT_FILE${NC}"
cat "$RESULT_FILE" | jq .

# Cleanup
cd /tmp
rm -rf "$WORK_DIR"

echo -e "${GREEN}=== Benchmark Complete ===${NC}"
