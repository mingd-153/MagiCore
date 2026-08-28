#!/bin/bash
# Monitor MagiCore Benchmark Suite Progress
# Run: ./monitor_suite.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHMARK_ROOT="$(dirname "$SCRIPT_DIR")"
RESULTS_DIR="$BENCHMARK_ROOT/results"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${BLUE}=== MagiCore Benchmark Suite Monitor ===${NC}"
echo ""

# Find latest log
LATEST_LOG=$(ls -t "$RESULTS_DIR"/suite_*.log 2>/dev/null | head -1)

if [ -z "$LATEST_LOG" ]; then
    echo -e "${YELLOW}No active suite log found${NC}"
    echo "Start suite: ./scripts/run_full_suite_native.sh"
    exit 1
fi

echo -e "${CYAN}Monitoring: $(basename "$LATEST_LOG")${NC}"
echo ""

# Count results
TOTAL_EXPECTED=20
COMPLETED=$(ls "$RESULTS_DIR"/*_run*_*.json 2>/dev/null | wc -l | xargs)
PERCENT=$((COMPLETED * 100 / TOTAL_EXPECTED))

echo -e "${GREEN}Progress: $COMPLETED/$TOTAL_EXPECTED runs ($PERCENT%)${NC}"

# Breakdown by PM
echo ""
echo -e "${CYAN}Breakdown by PM:${NC}"
for PM in npm pnpm bun mgc; do
    COUNT=$(ls "$RESULTS_DIR"/${PM}_run*_*.json 2>/dev/null | wc -l | xargs)
    printf "  %-6s: %d/5 runs\n" "$PM" "$COUNT"
done

# Latest result
echo ""
echo -e "${CYAN}Latest result:${NC}"
LATEST_RESULT=$(ls -t "$RESULTS_DIR"/*_run*_*.json 2>/dev/null | head -1)
if [ -n "$LATEST_RESULT" ]; then
    echo "  $(basename "$LATEST_RESULT")"
    PM=$(jq -r '.pm' "$LATEST_RESULT")
    COLD=$(jq -r '.cold_install.duration_seconds' "$LATEST_RESULT")
    DISK=$(jq -r '.cold_install.disk_mb' "$LATEST_RESULT")
    echo "  $PM: ${COLD}s cold, ${DISK}MB disk"
fi

# Suite timing
echo ""
echo -e "${CYAN}Suite timing:${NC}"
SUITE_START=$(stat -f "%Sm" -t "%Y-%m-%d %H:%M:%S" "$LATEST_LOG" 2>/dev/null || echo "unknown")
echo "  Started: $SUITE_START"

# Estimate completion
if [ $COMPLETED -gt 0 ]; then
    SUITE_START_EPOCH=$(stat -f "%m" "$LATEST_LOG" 2>/dev/null || echo "0")
    NOW_EPOCH=$(date +%s)
    ELAPSED=$((NOW_EPOCH - SUITE_START_EPOCH))
    
    if [ $ELAPSED -gt 0 ]; then
        HOURS=$((ELAPSED / 3600))
        MINUTES=$(((ELAPSED % 3600) / 60))
        echo "  Elapsed: ${HOURS}h ${MINUTES}m"
        
        # Estimate remaining
        AVG_PER_RUN=$((ELAPSED / COMPLETED))
        REMAINING_RUNS=$((TOTAL_EXPECTED - COMPLETED))
        REMAINING_SECONDS=$((AVG_PER_RUN * REMAINING_RUNS))
        REMAINING_HOURS=$((REMAINING_SECONDS / 3600))
        REMAINING_MINUTES=$(((REMAINING_SECONDS % 3600) / 60))
        
        echo "  Estimated remaining: ${REMAINING_HOURS}h ${REMAINING_MINUTES}m"
        
        EST_COMPLETE=$((NOW_EPOCH + REMAINING_SECONDS))
        EST_COMPLETE_STR=$(date -r $EST_COMPLETE "+%Y-%m-%d %H:%M:%S" 2>/dev/null || echo "unknown")
        echo "  Estimated completion: $EST_COMPLETE_STR"
    fi
fi

echo ""
echo -e "${YELLOW}Commands:${NC}"
echo "  Watch log: tail -f $LATEST_LOG"
echo "  List results: ls -lh $RESULTS_DIR/*.json"
echo "  Monitor again: ./scripts/monitor_suite.sh"
echo ""

exit 0
