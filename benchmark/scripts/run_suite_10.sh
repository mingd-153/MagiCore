#!/bin/bash
# Quick 10-run suite (enough for statistical validation)
set -euo pipefail

PM_NAME="${1:-mgc}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== 10-Run Suite: $PM_NAME ==="
echo "Start time: $(date)"

for i in {1..10}; do
  echo ""
  echo "[Run $i/10] Starting $PM_NAME benchmark..."
  
  "$SCRIPT_DIR/run_benchmark.sh" "$PM_NAME" "$i" || {
    echo "⚠️  Run $i failed (exit code $?)"
  }
  
  echo "✅ Run $i completed"
  
  if [ $i -lt 10 ]; then
    echo "Sleeping 30s before next run..."
    sleep 30
  fi
done

echo ""
echo "=== Suite Complete ==="
echo "End time: $(date)"
echo "Results: $SCRIPT_DIR/../results/${PM_NAME}_run*.json"
echo "Next steps:"
echo "1. Run analysis: python3 $SCRIPT_DIR/analyze_results.py $PM_NAME $SCRIPT_DIR/../results/"
