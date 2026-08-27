#!/bin/bash
echo "=== pnpm + mgc Suite Progress ==="
echo ""
echo "Results so far:"
cd benchmark/results
echo "  pnpm: $(ls pnpm_run[1-5]_*.json 2>/dev/null | wc -l | xargs)/5"
echo "  mgc: $(ls mgc_run[1-5]_*.json 2>/dev/null | wc -l | xargs)/5"
echo ""
echo "Latest:"
ls -t *.json 2>/dev/null | head -1
echo ""
echo "ETA: ~5-10 min total"
