#!/bin/bash
# MagiCore Full Benchmark Suite — 20 Runs (5 per PM)
# Execution time: 4-6 hours
# Run: ./run_full_suite.sh

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/../results"
LOG_FILE="$RESULTS_DIR/suite_$(date +%Y%m%d_%H%M%S).log"

mkdir -p "$RESULTS_DIR"

echo -e "${BLUE}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  MagiCore Full Benchmark Suite — V1.0 Launch      ║${NC}"
echo -e "${BLUE}║  20 Runs: 5 × mgc, npm, pnpm, bun                  ║${NC}"
echo -e "${BLUE}║  Estimated time: 4-6 hours                         ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════╝${NC}"
echo ""

# Package managers to test
PMS=("npm" "pnpm" "bun" "mgc")
RUNS_PER_PM=5
TOTAL_RUNS=$((${#PMS[@]} * RUNS_PER_PM))

echo -e "${YELLOW}Configuration:${NC}"
echo "  PMs: ${PMS[*]}"
echo "  Runs per PM: $RUNS_PER_PM"
echo "  Total runs: $TOTAL_RUNS"
echo "  Log file: $LOG_FILE"
echo ""

# Confirm execution
read -p "$(echo -e ${YELLOW}Start full benchmark suite? [y/N]: ${NC})" -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${RED}Aborted by user${NC}"
    exit 1
fi

START_SUITE=$(date +%s)
COMPLETED=0
FAILED=0

# Run benchmarks
for PM in "${PMS[@]}"; do
    echo ""
    echo -e "${GREEN}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║  PM: $PM (${RUNS_PER_PM} runs)                              ║${NC}"
    echo -e "${GREEN}╚════════════════════════════════════════════════════╝${NC}"
    echo ""
    
    for RUN in $(seq 1 $RUNS_PER_PM); do
        CURRENT=$((COMPLETED + 1))
        echo -e "${BLUE}>>> Run $CURRENT/$TOTAL_RUNS: $PM #$RUN${NC}"
        echo "Started: $(date)" | tee -a "$LOG_FILE"
        
        # Run in Docker
        if docker compose -f "$SCRIPT_DIR/../docker-compose.yml" run --rm benchmark \
            /benchmark/scripts/run_benchmark.sh "$PM" "$RUN" 2>&1 | tee -a "$LOG_FILE"; then
            echo -e "${GREEN}✓ $PM run $RUN completed${NC}" | tee -a "$LOG_FILE"
            COMPLETED=$((COMPLETED + 1))
        else
            echo -e "${RED}✗ $PM run $RUN FAILED${NC}" | tee -a "$LOG_FILE"
            FAILED=$((FAILED + 1))
            
            # Continue on failure (network issues, timeouts, etc.)
            echo -e "${YELLOW}Continuing despite failure...${NC}"
        fi
        
        # Progress
        PERCENT=$((CURRENT * 100 / TOTAL_RUNS))
        echo -e "${BLUE}Progress: $CURRENT/$TOTAL_RUNS ($PERCENT%)${NC}"
        
        # Cooldown between runs (avoid thermal throttling)
        if [ $CURRENT -lt $TOTAL_RUNS ]; then
            echo -e "${YELLOW}Cooldown 30s...${NC}"
            sleep 30
        fi
    done
done

END_SUITE=$(date +%s)
SUITE_DURATION=$((END_SUITE - START_SUITE))
HOURS=$((SUITE_DURATION / 3600))
MINUTES=$(((SUITE_DURATION % 3600) / 60))

echo ""
echo -e "${GREEN}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║  Benchmark Suite Complete                          ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${BLUE}Results:${NC}"
echo "  Completed: $COMPLETED/$TOTAL_RUNS"
echo "  Failed: $FAILED"
echo "  Duration: ${HOURS}h ${MINUTES}m"
echo "  Log: $LOG_FILE"
echo ""

# Analyze results
echo -e "${YELLOW}Analyzing results...${NC}"
ANALYZE_SCRIPT="$SCRIPT_DIR/analyze_results.py"

if [ -f "$ANALYZE_SCRIPT" ]; then
    python3 "$ANALYZE_SCRIPT" "$RESULTS_DIR"
else
    echo -e "${YELLOW}Note: analyze_results.py not found, skipping analysis${NC}"
    echo "  Manually analyze: ls -lh $RESULTS_DIR/*.json"
fi

echo ""
echo -e "${GREEN}✓ Full benchmark suite finished!${NC}"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "  1. Review results: ls $RESULTS_DIR/"
echo "  2. Update BENCHMARK.md with final data"
echo "  3. Remove '🚧 PRELIMINARY' status"
echo "  4. Commit + push for launch"
echo ""

exit 0
