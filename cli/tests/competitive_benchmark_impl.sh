#!/usr/bin/env bash
# Competitive Benchmark — mgc vs pnpm (10 runs, median, p95, JSON output)
# Status: STATISTICAL IMPLEMENTATION (real data collection)

set -euo pipefail

echo "=== Competitive Benchmark: mgc vs pnpm (Statistical) ==="
echo

# Check dependencies
if ! command -v pnpm &>/dev/null; then
    echo "⚠️  SKIP: pnpm not installed (cannot compare)"
    exit 77
fi

if ! command -v bc &>/dev/null; then
    echo "⚠️  SKIP: bc not installed (needed for calculations)"
    exit 77
fi

# Find mgc binary (REQUIRED)
PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
if [ -f "$PROJECT_ROOT/target/release/mgc" ]; then
    MGC_BIN="$PROJECT_ROOT/target/release/mgc"
elif [ -f "$PROJECT_ROOT/target/debug/mgc" ]; then
    MGC_BIN="$PROJECT_ROOT/target/debug/mgc"
elif command -v mgc &>/dev/null; then
    MGC_BIN="mgc"
else
    echo "✗ FAIL: mgc binary not found"
    exit 1
fi

echo "Using mgc: $MGC_BIN"
MGC_VERSION=$("$MGC_BIN" --version 2>/dev/null || echo "unknown")
PNPM_VERSION=$(pnpm --version 2>/dev/null || echo "unknown")
echo "mgc version: $MGC_VERSION"
echo "pnpm version: $PNPM_VERSION"
echo

# Hardware info
OS=$(uname -s)
ARCH=$(uname -m)
CPU_CORES=$(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo "unknown")
TOTAL_RAM=$(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1024 / 1024 / 1024 ))
echo "Hardware: $OS $ARCH, ${CPU_CORES} cores, ${TOTAL_RAM}GB RAM"
echo

# Create temp workspaces
TEMP_BASE=$(mktemp -d)
trap "rm -rf $TEMP_BASE" EXIT

# Test manifest (small but realistic)
TEST_MANIFEST=$(cat <<'EOF'
{
  "name": "benchmark-test",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "lodash": "^4.17.21"
  }
}
EOF
)

NUM_RUNS=10
echo "Running $NUM_RUNS iterations for statistical significance..."
echo

# Arrays to store results
declare -a PNPM_TIMES=()
declare -a MGC_TIMES=()

# Run benchmarks
for i in $(seq 1 $NUM_RUNS); do
    echo "--- Run $i/$NUM_RUNS ---"
    
    # pnpm
    PNPM_DIR="$TEMP_BASE/pnpm-run-$i"
    mkdir -p "$PNPM_DIR"
    echo "$TEST_MANIFEST" > "$PNPM_DIR/package.json"
    cd "$PNPM_DIR"
    
    pnpm store prune --force >/dev/null 2>&1 || true
    START=$(date +%s%N)
    if pnpm install --no-lockfile --force >/dev/null 2>&1; then
        END=$(date +%s%N)
        DURATION=$(( (END - START) / 1000000 ))
        PNPM_TIMES+=("$DURATION")
        echo "  pnpm: ${DURATION}ms"
    else
        echo "  pnpm: FAILED"
        PNPM_TIMES+=("-1")
    fi
    
    # mgc
    MGC_DIR="$TEMP_BASE/mgc-run-$i"
    mkdir -p "$MGC_DIR"
    echo "$TEST_MANIFEST" > "$MGC_DIR/package.json"
    cd "$MGC_DIR"
    
    # Clear mgc cache
    rm -rf ~/.magicore/store ~/.mgc/cache 2>/dev/null || true
    START=$(date +%s%N)
    if "$MGC_BIN" install --force >/dev/null 2>&1; then
        END=$(date +%s%N)
        DURATION=$(( (END - START) / 1000000 ))
        MGC_TIMES+=("$DURATION")
        echo "  mgc:  ${DURATION}ms"
    else
        echo "  mgc:  FAILED"
        MGC_TIMES+=("-1")
    fi
    
    echo
done

# Calculate statistics
calc_median() {
    local arr=("$@")
    local sorted=($(printf '%s\n' "${arr[@]}" | sort -n))
    local len=${#sorted[@]}
    local mid=$(( len / 2 ))
    
    if [ $(( len % 2 )) -eq 0 ]; then
        # Even: average of two middle values
        echo $(( (sorted[mid-1] + sorted[mid]) / 2 ))
    else
        # Odd: middle value
        echo "${sorted[mid]}"
    fi
}

calc_p95() {
    local arr=("$@")
    local sorted=($(printf '%s\n' "${arr[@]}" | sort -n))
    local len=${#sorted[@]}
    local idx=$(( len * 95 / 100 ))
    [ $idx -ge $len ] && idx=$(( len - 1 ))
    echo "${sorted[idx]}"
}

PNPM_MEDIAN=$(calc_median "${PNPM_TIMES[@]}")
PNPM_P95=$(calc_p95 "${PNPM_TIMES[@]}")
MGC_MEDIAN=$(calc_median "${MGC_TIMES[@]}")
MGC_P95=$(calc_p95 "${MGC_TIMES[@]}")

echo "=== Results ==="
echo "pnpm: median=${PNPM_MEDIAN}ms, p95=${PNPM_P95}ms"
echo "mgc:  median=${MGC_MEDIAN}ms, p95=${MGC_P95}ms"
echo

# Calculate speedup
if [ "$PNPM_MEDIAN" -gt 0 ] && [ "$MGC_MEDIAN" -gt 0 ]; then
    SPEEDUP=$(echo "scale=2; $PNPM_MEDIAN / $MGC_MEDIAN" | bc)
    if (( $(echo "$SPEEDUP > 1" | bc -l) )); then
        echo "Result: mgc ${SPEEDUP}x faster than pnpm (median)"
    else
        SLOWDOWN=$(echo "scale=2; $MGC_MEDIAN / $PNPM_MEDIAN" | bc)
        echo "Result: mgc ${SLOWDOWN}x slower than pnpm (median)"
    fi
fi

# Generate JSON output
JSON_FILE="$TEMP_BASE/benchmark_results.json"
cat > "$JSON_FILE" <<EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "hardware": {
    "os": "$OS",
    "arch": "$ARCH",
    "cpu_cores": $CPU_CORES,
    "total_ram_gb": $TOTAL_RAM
  },
  "tools": {
    "mgc": {
      "version": "$MGC_VERSION",
      "binary": "$MGC_BIN"
    },
    "pnpm": {
      "version": "$PNPM_VERSION"
    }
  },
  "workload": {
    "dependencies": 3,
    "description": "react + react-dom + lodash"
  },
  "methodology": {
    "runs": $NUM_RUNS,
    "cache_cleared": true,
    "metrics": ["median", "p95"]
  },
  "results": {
    "pnpm": {
      "raw_times_ms": [$(IFS=,; echo "${PNPM_TIMES[*]}")],
      "median_ms": $PNPM_MEDIAN,
      "p95_ms": $PNPM_P95
    },
    "mgc": {
      "raw_times_ms": [$(IFS=,; echo "${MGC_TIMES[*]}")],
      "median_ms": $MGC_MEDIAN,
      "p95_ms": $MGC_P95
    }
  }
}
EOF

echo
echo "✓ JSON output: $JSON_FILE"
cat "$JSON_FILE"
echo
echo
echo "--- Caveats ---"
echo "• Small workload (3 dependencies)"
echo "• Statistical: 10 runs, median + p95"
echo "• Hardware profiling: basic (OS/arch/cores/RAM)"
echo "• JSON output generated"
echo "• Limited to pnpm comparison only"
echo
echo "For production claims, still need:"
echo "  - Multiple competitors (bun/deno/moon/proto)"
echo "  - Larger workloads (100+ dependencies)"
echo "  - Detailed hardware profiling (disk I/O, network)"
echo "  - Fresh environment per run (Docker/VM)"

# Exit with success if we got valid measurements
if [ "$PNPM_MEDIAN" -gt 0 ] && [ "$MGC_MEDIAN" -gt 0 ]; then
    echo
    echo "✓ PASS: Statistical competitive benchmark complete"
    exit 0
else
    echo
    echo "✗ FAIL: Benchmark measurements invalid"
    exit 1
fi
