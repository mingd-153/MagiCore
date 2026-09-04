#!/bin/bash
# Quick 5-run suite for workload validation
set -euo pipefail

PM_NAME="${1:-mgc}"
WORKLOAD="${2:-package-small.json}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== Quick 5-Run Suite: $PM_NAME on $WORKLOAD ==="
echo "Start time: $(date)"

for i in {1..5}; do
  echo "[Run $i/5] Starting $PM_NAME benchmark..."
  
  # Run benchmark with specific workload
  PACKAGE_JSON="$SCRIPT_DIR/../env/$WORKLOAD" \
    "$SCRIPT_DIR/run_benchmark.sh" "$PM_NAME" "$i" || {
    echo "⚠️  Run $i failed (exit code $?)"
  }
  
  echo "✅ Run $i completed"
  
  # Short sleep between runs
  if [ $i -lt 5 ]; then
    echo "Sleeping 10s before next run..."
    sleep 10
  fi
done

echo "=== Suite Complete ==="
echo "End time: $(date)"
echo "Results: $SCRIPT_DIR/../results/${PM_NAME}_run*.json"
echo "Next steps:"
echo "1. Run analysis: python3 $SCRIPT_DIR/analyze_results.py $PM_NAME $SCRIPT_DIR/../results/"
