#!/bin/bash
# Quick pnpm + mgc benchmark (10 runs: 5 each)
# After fixing script issues

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$(dirname "$SCRIPT_DIR")/results"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  pnpm + mgc Benchmark (10 runs: 5 each)           ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════╝${NC}"
echo ""

START=$(date +%s)
TOTAL=0
FAILED=0

# pnpm runs
echo -e "${GREEN}Running pnpm (5 runs)...${NC}"
for i in {1..5}; do
    echo -e "${YELLOW}pnpm run $i/5${NC}"
    if "$SCRIPT_DIR/run_benchmark_native.sh" pnpm "$i"; then
        TOTAL=$((TOTAL + 1))
    else
        FAILED=$((FAILED + 1))
    fi
    [ $i -lt 5 ] && sleep 10
done

echo ""
echo -e "${GREEN}Running mgc (5 runs)...${NC}"
for i in {1..5}; do
    echo -e "${YELLOW}mgc run $i/5${NC}"
    if "$SCRIPT_DIR/run_benchmark_native.sh" mgc "$i"; then
        TOTAL=$((TOTAL + 1))
    else
        FAILED=$((FAILED + 1))
    fi
    [ $i -lt 5 ] && sleep 10
done

END=$(date +%s)
DURATION=$((END - START))

echo ""
echo -e "${GREEN}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║  Suite Complete!                                   ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════╝${NC}"
echo ""
echo "Completed: $TOTAL/10"
echo "Failed: $FAILED"
echo "Duration: ${DURATION}s"
echo ""
echo "Analyzing..."
python3 "$SCRIPT_DIR/analyze_results.py" "$RESULTS_DIR"
