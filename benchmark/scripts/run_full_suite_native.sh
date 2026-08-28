#!/bin/bash
# MagiCore Full Benchmark Suite — Native macOS (No Docker)
# 20 Runs: 5 runs × 4 PMs (npm, pnpm, bun, mgc)
# Estimated time: 4-6 hours
# Run: ./run_full_suite_native.sh

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHMARK_ROOT="$(dirname "$SCRIPT_DIR")"
RESULTS_DIR="$BENCHMARK_ROOT/results"
LOG_FILE="$RESULTS_DIR/suite_$(date +%Y%m%d_%H%M%S).log"

mkdir -p "$RESULTS_DIR"

echo -e "${BLUE}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  MagiCore Full Benchmark Suite — V1.0 Launch      ║${NC}"
echo -e "${BLUE}║  Native macOS Execution (No Docker)                ║${NC}"
echo -e "${BLUE}║  20 Runs: 5 × npm, pnpm, bun, mgc                  ║${NC}"
echo -e "${BLUE}║  Estimated time: 4-6 hours                         ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════╝${NC}"
echo ""

# Configuration
PMS=("npm" "pnpm" "bun" "mgc")
RUNS_PER_PM=5
TOTAL_RUNS=$((${#PMS[@]} * RUNS_PER_PM))

echo -e "${CYAN}Configuration:${NC}"
echo "  PMs: ${PMS[*]}"
echo "  Runs per PM: $RUNS_PER_PM"
echo "  Total runs: $TOTAL_RUNS"
echo "  Results: $RESULTS_DIR"
echo "  Log: $LOG_FILE"
echo ""

# Prerequisites check
echo -e "${YELLOW}Checking prerequisites...${NC}"

# Check jq
if ! command -v jq &> /dev/null; then
    echo -e "${RED}✗ jq not installed${NC}"
    echo "Install: brew install jq"
    exit 1
fi
echo -e "${GREEN}✓ jq available${NC}"

# Check bc (for floating point math)
if ! command -v bc &> /dev/null; then
    echo -e "${RED}✗ bc not installed${NC}"
    echo "Install: brew install bc"
    exit 1
fi
echo -e "${GREEN}✓ bc available${NC}"

# Check gdate (GNU date for nanoseconds)
if ! command -v gdate &> /dev/null; then
    echo -e "${YELLOW}⚠ gdate not installed (fallback to date)${NC}"
    echo "For better precision: brew install coreutils"
fi

# Check PMs
for PM in "${PMS[@]}"; do
    case "$PM" in
        mgc)
            MGC_BINARY="$BENCHMARK_ROOT/../target/release/mgc"
            if [ ! -f "$MGC_BINARY" ]; then
                echo -e "${RED}✗ mgc binary not found${NC}"
                echo "  Expected: $MGC_BINARY"
                echo "  Run: cargo build --release"
                exit 1
            fi
            echo -e "${GREEN}✓ mgc: $MGC_BINARY${NC}"
            ;;
        pnpm)
            if ! command -v pnpm &> /dev/null; then
                echo -e "${YELLOW}⚠ pnpm not installed, skipping${NC}"
                # Remove from array
                PMS=("${PMS[@]/pnpm/}")
            else
                echo -e "${GREEN}✓ pnpm: $(command -v pnpm)${NC}"
            fi
            ;;
        bun)
            if ! command -v bun &> /dev/null; then
                echo -e "${YELLOW}⚠ bun not installed, skipping${NC}"
                PMS=("${PMS[@]/bun/}")
            else
                echo -e "${GREEN}✓ bun: $(command -v bun)${NC}"
            fi
            ;;
        npm)
            if ! command -v npm &> /dev/null; then
                echo -e "${RED}✗ npm not installed${NC}"
                exit 1
            fi
            echo -e "${GREEN}✓ npm: $(command -v npm)${NC}"
            ;;
    esac
done

# Remove empty elements from array
PMS=("${PMS[@]}")
TOTAL_RUNS=$((${#PMS[@]} * RUNS_PER_PM))

echo ""
echo -e "${CYAN}Actual configuration after checks:${NC}"
echo "  PMs to test: ${PMS[*]}"
echo "  Total runs: $TOTAL_RUNS"
echo ""

# Confirm execution
read -p "$(echo -e ${YELLOW}Start full benchmark suite? This will take 4-6 hours. [y/N]: ${NC})" -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${RED}Aborted by user${NC}"
    exit 1
fi

echo "" | tee -a "$LOG_FILE"
echo "=== MagiCore Benchmark Suite Started ===" | tee -a "$LOG_FILE"
echo "Date: $(date)" | tee -a "$LOG_FILE"
echo "Machine: $(sysctl -n machdep.cpu.brand_string)" | tee -a "$LOG_FILE"
echo "Cores: $(sysctl -n hw.ncpu)" | tee -a "$LOG_FILE"
echo "Memory: $(echo "$(sysctl -n hw.memsize) / 1024 / 1024 / 1024" | bc)GB" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

START_SUITE=$(date +%s)
COMPLETED=0
FAILED=0
FAILED_RUNS=()

# Main benchmark loop
for PM in "${PMS[@]}"; do
    # Skip empty elements
    if [ -z "$PM" ]; then
        continue
    fi
    
    echo "" | tee -a "$LOG_FILE"
    echo -e "${GREEN}╔════════════════════════════════════════════════════╗${NC}" | tee -a "$LOG_FILE"
    echo -e "${GREEN}║  PM: $PM (${RUNS_PER_PM} runs)                              ║${NC}" | tee -a "$LOG_FILE"
    echo -e "${GREEN}╚════════════════════════════════════════════════════╝${NC}" | tee -a "$LOG_FILE"
    echo "" | tee -a "$LOG_FILE"
    
    for RUN in $(seq 1 $RUNS_PER_PM); do
        CURRENT=$((COMPLETED + FAILED + 1))
        PERCENT=$((CURRENT * 100 / TOTAL_RUNS))
        
        echo -e "${BLUE}┌─────────────────────────────────────────────────┐${NC}" | tee -a "$LOG_FILE"
        echo -e "${BLUE}│ Run $CURRENT/$TOTAL_RUNS ($PERCENT%): $PM #$RUN                       │${NC}" | tee -a "$LOG_FILE"
        echo -e "${BLUE}└─────────────────────────────────────────────────┘${NC}" | tee -a "$LOG_FILE"
        echo "Started: $(date)" | tee -a "$LOG_FILE"
        
        # Run benchmark
        RUN_START=$(date +%s)
        if "$SCRIPT_DIR/run_benchmark_native.sh" "$PM" "$RUN" 2>&1 | tee -a "$LOG_FILE"; then
            RUN_END=$(date +%s)
            RUN_DURATION=$((RUN_END - RUN_START))
            
            echo -e "${GREEN}✓ $PM run $RUN completed in ${RUN_DURATION}s${NC}" | tee -a "$LOG_FILE"
            COMPLETED=$((COMPLETED + 1))
        else
            RUN_END=$(date +%s)
            RUN_DURATION=$((RUN_END - RUN_START))
            
            echo -e "${RED}✗ $PM run $RUN FAILED after ${RUN_DURATION}s${NC}" | tee -a "$LOG_FILE"
            FAILED=$((FAILED + 1))
            FAILED_RUNS+=("$PM run $RUN")
            
            # Continue despite failure
            echo -e "${YELLOW}Continuing despite failure...${NC}" | tee -a "$LOG_FILE"
        fi
        
        # Progress bar
        FILLED=$((CURRENT * 50 / TOTAL_RUNS))
        EMPTY=$((50 - FILLED))
        printf "${CYAN}Progress: [" | tee -a "$LOG_FILE"
        printf "%${FILLED}s" | tr ' ' '=' | tee -a "$LOG_FILE"
        printf "%${EMPTY}s" | tee -a "$LOG_FILE"
        printf "] $CURRENT/$TOTAL_RUNS${NC}\n" | tee -a "$LOG_FILE"
        
        # Cooldown between runs (prevent thermal throttling)
        if [ $CURRENT -lt $TOTAL_RUNS ]; then
            echo -e "${YELLOW}Cooldown 30s (thermal stability)...${NC}" | tee -a "$LOG_FILE"
            sleep 30
        fi
        
        echo "" | tee -a "$LOG_FILE"
    done
done

END_SUITE=$(date +%s)
SUITE_DURATION=$((END_SUITE - START_SUITE))
HOURS=$((SUITE_DURATION / 3600))
MINUTES=$(((SUITE_DURATION % 3600) / 60))
SECONDS=$((SUITE_DURATION % 60))

echo "" | tee -a "$LOG_FILE"
echo -e "${GREEN}╔════════════════════════════════════════════════════╗${NC}" | tee -a "$LOG_FILE"
echo -e "${GREEN}║  Benchmark Suite Complete!                         ║${NC}" | tee -a "$LOG_FILE"
echo -e "${GREEN}╚════════════════════════════════════════════════════╝${NC}" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

echo -e "${CYAN}Results Summary:${NC}" | tee -a "$LOG_FILE"
echo "  Completed: $COMPLETED/$TOTAL_RUNS" | tee -a "$LOG_FILE"
echo "  Failed: $FAILED" | tee -a "$LOG_FILE"
echo "  Duration: ${HOURS}h ${MINUTES}m ${SECONDS}s" | tee -a "$LOG_FILE"
echo "  Results directory: $RESULTS_DIR" | tee -a "$LOG_FILE"
echo "  Log file: $LOG_FILE" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

if [ $FAILED -gt 0 ]; then
    echo -e "${YELLOW}Failed runs:${NC}" | tee -a "$LOG_FILE"
    for FAILED_RUN in "${FAILED_RUNS[@]}"; do
        echo "  - $FAILED_RUN" | tee -a "$LOG_FILE"
    done
    echo "" | tee -a "$LOG_FILE"
fi

# Analyze results
echo -e "${YELLOW}Analyzing results...${NC}" | tee -a "$LOG_FILE"
ANALYZE_SCRIPT="$SCRIPT_DIR/analyze_results.py"

if [ -f "$ANALYZE_SCRIPT" ]; then
    if python3 "$ANALYZE_SCRIPT" "$RESULTS_DIR" 2>&1 | tee -a "$LOG_FILE"; then
        echo -e "${GREEN}✓ Analysis complete${NC}" | tee -a "$LOG_FILE"
    else
        echo -e "${YELLOW}⚠ Analysis script failed, skipping${NC}" | tee -a "$LOG_FILE"
    fi
else
    echo -e "${YELLOW}⚠ analyze_results.py not found${NC}" | tee -a "$LOG_FILE"
    echo "  Manually analyze: ls $RESULTS_DIR/*.json" | tee -a "$LOG_FILE"
fi

echo "" | tee -a "$LOG_FILE"
echo -e "${GREEN}✓ Full benchmark suite finished!${NC}" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

echo -e "${CYAN}Next steps:${NC}"
echo "  1. Review results: ls -lh $RESULTS_DIR/*.json"
echo "  2. Read analysis output above (or in $LOG_FILE)"
echo "  3. Update docs/BENCHMARK.md with final data"
echo "  4. Remove '🚧 PRELIMINARY' status badge"
echo "  5. Commit + push for V1.0 launch"
echo ""

# Generate summary file
SUMMARY_FILE="$RESULTS_DIR/SUITE_SUMMARY_$(date +%Y%m%d_%H%M%S).txt"
cat > "$SUMMARY_FILE" <<EOF
MagiCore Benchmark Suite Summary
Date: $(date)
Duration: ${HOURS}h ${MINUTES}m ${SECONDS}s
Total Runs: $TOTAL_RUNS
Completed: $COMPLETED
Failed: $FAILED
Success Rate: $((COMPLETED * 100 / TOTAL_RUNS))%

PMs Tested: ${PMS[*]}
Runs Per PM: $RUNS_PER_PM

Machine Spec:
  CPU: $(sysctl -n machdep.cpu.brand_string)
  Cores: $(sysctl -n hw.ncpu)
  Memory: $(echo "$(sysctl -n hw.memsize) / 1024 / 1024 / 1024" | bc)GB
  OS: $(uname -s) $(uname -r)
  Node: $(node --version 2>/dev/null || echo "N/A")

Results: $RESULTS_DIR
Log: $LOG_FILE
EOF

echo -e "${GREEN}✓ Summary saved: $SUMMARY_FILE${NC}"
echo ""

exit 0
