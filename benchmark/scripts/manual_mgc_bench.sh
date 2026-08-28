#!/bin/bash
# Manual mgc benchmark with 20-package set (Next.js)

set -euo pipefail

MGC_BINARY="/Users/doanmihh/Documents/Workspace/MagiCore/target/release/mgc"
PACKAGE_JSON="/Users/doanmihh/Documents/Workspace/MagiCore/benchmark/env/package.json"

echo "=== MGC Benchmark (20 packages with Next.js) ==="
echo "Running 5 cold + warm cycles..."
echo ""

for i in {1..5}; do
    echo "=== Run $i/5 ==="
    
    # Create clean workspace
    WORK_DIR="/tmp/mgc_bench_run${i}_$(date +%Y%m%d_%H%M%S)"
    mkdir -p "$WORK_DIR"
    cp "$PACKAGE_JSON" "$WORK_DIR/package.json"
    cd "$WORK_DIR"
    
    # Clean cache
    rm -rf ~/.magicore/store ~/.magicore/cache 2>/dev/null || true
    
    # COLD install
    echo "  Cold install..."
    START=$(gdate +%s.%N)
    $MGC_BINARY install-web > install.log 2>&1
    END=$(gdate +%s.%N)
    COLD_TIME=$(echo "$END - $START" | bc)
    
    # Get disk size
    DISK_MB=$(du -sm node_modules | cut -f1)
    
    # Get package count
    PKG_COUNT=$(ls node_modules | wc -l | tr -d ' ')
    
    # WARM install (re-install with cache)
    echo "  Warm install..."
    rm -rf node_modules package-lock.json mgc.lock 2>/dev/null || true
    START=$(gdate +%s.%N)
    $MGC_BINARY install-web > install_warm.log 2>&1
    END=$(gdate +%s.%N)
    WARM_TIME=$(echo "$END - $START" | bc)
    
    # Save JSON result
    RESULT_FILE="/Users/doanmihh/Documents/Workspace/MagiCore/benchmark/results/mgc_v2_run${i}_$(date +%Y%m%d_%H%M%S).json"
    
    cat > "$RESULT_FILE" << EOF
{
  "pm": "mgc",
  "run": $i,
  "timestamp": "$(date +%Y%m%d_%H%M%S)",
  "machine": {
    "cpu": "Apple M2",
    "cores": 8,
    "memory_gb": 16,
    "os": "Darwin 25.5.0",
    "node_version": "v25.9.0",
    "timestamp": "$(date +%Y%m%d_%H%M%S)"
  },
  "cold_install": {
    "duration_seconds": "$COLD_TIME",
    "disk_mb": $DISK_MB
  },
  "warm_install": {
    "duration_seconds": "$WARM_TIME"
  },
  "package_count": $PKG_COUNT,
  "notes": "G1 fix applied - wildcard ranges working, includes Next.js"
}
EOF
    
    echo "  ✓ Cold: ${COLD_TIME}s | Warm: ${WARM_TIME}s | Disk: ${DISK_MB}MB | Packages: $PKG_COUNT"
    echo "  Saved to: $RESULT_FILE"
    echo ""
    
    # Cleanup
    cd /
    rm -rf "$WORK_DIR"
done

echo "=== Benchmark Complete! ==="
echo "Results saved to: benchmark/results/mgc_v2_run*.json"
