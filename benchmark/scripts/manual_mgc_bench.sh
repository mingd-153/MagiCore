#!/bin/bash
# Manual mgc benchmark with 20-package set (Next.js)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MGC_BINARY="${MGC_BINARY:-$PROJECT_ROOT/target/release/mgc}"
PACKAGE_JSON="${PACKAGE_JSON:-$PROJECT_ROOT/benchmark/env/package.json}"
RESULTS_DIR="${RESULTS_DIR:-$PROJECT_ROOT/benchmark/results}"
ISOLATED_HOME="$(mktemp -d "${TMPDIR:-/tmp}/mgc-bench-home.XXXXXX")"
export HOME="$ISOLATED_HOME"
export XDG_CACHE_HOME="$ISOLATED_HOME/.cache"
export MGC_CACHE_DIR="$ISOLATED_HOME/.mgc"

cleanup() {
    rm -rf "$ISOLATED_HOME"
}
trap cleanup EXIT

now_seconds() {
    if command -v python3 >/dev/null 2>&1; then
        python3 -c 'import time; print(f"{time.time_ns() / 1_000_000_000:.9f}")'
    elif command -v gdate >/dev/null 2>&1; then
        gdate +%s.%N
    else
        date +%s
    fi
}

cpu_model() {
    if command -v sysctl >/dev/null 2>&1; then
        sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -m
    elif command -v lscpu >/dev/null 2>&1; then
        lscpu | sed -n 's/^Model name:[[:space:]]*//p' | head -n 1
    else
        uname -m
    fi
}

cpu_cores() {
    getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 0
}

memory_gb() {
    if command -v sysctl >/dev/null 2>&1; then
        bytes="$(sysctl -n hw.memsize 2>/dev/null || echo 0)"
        awk -v bytes="$bytes" 'BEGIN { printf "%.0f", bytes / 1073741824 }'
    elif command -v free >/dev/null 2>&1; then
        free -g | awk '/^Mem:/ { print $2; exit }'
    else
        echo 0
    fi
}

if [ ! -x "$MGC_BINARY" ]; then
    echo "mgc binary is not executable: $MGC_BINARY" >&2
    exit 1
fi
if [ ! -f "$PACKAGE_JSON" ]; then
    echo "benchmark manifest not found: $PACKAGE_JSON" >&2
    exit 1
fi
mkdir -p "$RESULTS_DIR"

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
    
    # Clean only the isolated benchmark cache; never touch the user's cache.
    # (Chỉ dọn cache riêng của benchmark, không đụng cache thật của người dùng.)
    rm -rf "$ISOLATED_HOME/.magicore" "$ISOLATED_HOME/.mgc"
    mkdir -p "$ISOLATED_HOME/.cache" "$ISOLATED_HOME/.mgc"
    
    # COLD install
    echo "  Cold install..."
    START=$(now_seconds)
    "$MGC_BINARY" install-web > install.log 2>&1
    END=$(now_seconds)
    COLD_TIME=$(echo "$END - $START" | bc)
    
    # Get disk size
    DISK_MB=$(du -sm node_modules | cut -f1)
    
    # Get package count
    PKG_COUNT=$(ls node_modules | wc -l | tr -d ' ')
    
    # WARM install (re-install with cache)
    echo "  Warm install..."
    rm -rf node_modules package-lock.json mgc.lock 2>/dev/null || true
    START=$(now_seconds)
    "$MGC_BINARY" install-web > install_warm.log 2>&1
    END=$(now_seconds)
    WARM_TIME=$(echo "$END - $START" | bc)
    
    # Save JSON result
    RESULT_FILE="$RESULTS_DIR/mgc_v2_run${i}_$(date +%Y%m%d_%H%M%S).json"
    
    cat > "$RESULT_FILE" << EOF
{
  "pm": "mgc",
  "run": $i,
  "timestamp": "$(date +%Y%m%d_%H%M%S)",
  "machine": {
    "cpu": "$(cpu_model)",
    "cores": $(cpu_cores),
    "memory_gb": $(memory_gb),
    "os": "$(uname -s) $(uname -r)",
    "node_version": "$(node --version 2>/dev/null || echo 'N/A')",
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
    rm -rf "$WORK_DIR"
done

echo "=== Benchmark Complete! ==="
echo "Results saved to: benchmark/results/mgc_v2_run*.json"
