#!/bin/bash
# P1.1 FIX: Run 20 benchmark iterations per PM
# Requirement: 20-30 runs per workload with median/p95/stddev
# Run: ./run_suite_20.sh <pm_name>
# Example: ./run_suite_20.sh mgc

set -euo pipefail

PM_NAME="${1:-mgc}"
TOTAL_RUNS=20
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/../results/p1_suite"

mkdir -p "$RESULTS_DIR"

echo "=== P1.1 Benchmark Suite: 20 Runs for $PM_NAME ==="
echo "Start time: $(date)"
echo ""

# Run 20 iterations
for i in $(seq 1 $TOTAL_RUNS); do
    echo "[Run $i/$TOTAL_RUNS] Starting $PM_NAME benchmark..."

    # Call existing benchmark script
    if bash "$SCRIPT_DIR/run_benchmark.sh" "$PM_NAME" "$i" 2>&1 | tee "$RESULTS_DIR/${PM_NAME}_run${i}.log"; then
        echo "✅ Run $i completed"
    else
        echo "⚠️  Run $i failed (exit code $?)"
    fi

    # Sleep between runs to let system stabilize
    if [ $i -lt $TOTAL_RUNS ]; then
        echo "Sleeping 30s before next run..."
        sleep 30
    fi
done

echo ""
echo "=== Suite Complete ==="
echo "End time: $(date)"
echo "Results: $RESULTS_DIR/${PM_NAME}_run*.json"
echo ""
echo "Next steps:"
echo "1. Run analysis: ./analyze_results.py"
echo "2. Generate report: ./generate_report.sh"
