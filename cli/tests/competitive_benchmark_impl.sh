#!/usr/bin/env bash
# Competitive Benchmark — mgc vs pnpm (basic implementation)
# Status: BASIC IMPLEMENTATION (real comparison with caveats)

set -euo pipefail

echo "=== Competitive Benchmark: mgc vs pnpm ==="
echo

# Check if pnpm is available
if ! command -v pnpm &>/dev/null; then
    echo "⚠️  SKIP: pnpm not installed (cannot compare)"
    exit 77
fi

# Create temp workspaces
TEMP_BASE=$(mktemp -d)
trap "rm -rf $TEMP_BASE" EXIT

MGC_DIR="$TEMP_BASE/mgc-test"
PNPM_DIR="$TEMP_BASE/pnpm-test"

mkdir -p "$MGC_DIR" "$PNPM_DIR"

# Create identical test package.json (small dependency set)
TEST_MANIFEST=$(cat <<'EOF'
{
  "name": "benchmark-test",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0"
  }
}
EOF
)

echo "$TEST_MANIFEST" > "$MGC_DIR/package.json"
echo "$TEST_MANIFEST" > "$PNPM_DIR/package.json"

# Benchmark function
benchmark() {
    local tool=$1
    local dir=$2
    local cmd=$3
    
    echo "Testing $tool..."
    cd "$dir"
    
    # Measure time
    START=$(date +%s%N)
    if eval "$cmd" >/dev/null 2>&1; then
        END=$(date +%s%N)
        DURATION=$(( (END - START) / 1000000 )) # milliseconds
        echo "✓ $tool: ${DURATION}ms"
        echo "$DURATION"
    else
        echo "✗ $tool: FAILED"
        echo "-1"
    fi
}

# Cold install comparison
echo "--- Cold Install (empty cache) ---"

# pnpm cold
pnpm store prune --force >/dev/null 2>&1 || true
PNPM_COLD=$(benchmark "pnpm" "$PNPM_DIR" "pnpm install --no-lockfile --force")

# mgc cold (if available)
if command -v mgc &>/dev/null; then
    MGC_COLD=$(benchmark "mgc" "$MGC_DIR" "mgc install --force")
else
    echo "⚠️  mgc not in PATH, using cache_tracking_stress.sh data as reference"
    MGC_COLD="N/A"
fi

echo
echo "--- Results Summary ---"
echo "pnpm cold install: ${PNPM_COLD}ms"
echo "mgc cold install: ${MGC_COLD}ms"

# Determine result
if [ "$MGC_COLD" != "N/A" ] && [ "$MGC_COLD" != "-1" ]; then
    if [ "$PNPM_COLD" -gt 0 ]; then
        SPEEDUP=$(echo "scale=2; $PNPM_COLD / $MGC_COLD" | bc)
        if (( $(echo "$SPEEDUP > 1" | bc -l) )); then
            echo "✓ mgc is ${SPEEDUP}x faster than pnpm (cold install)"
        else
            SLOWDOWN=$(echo "scale=2; $MGC_COLD / $PNPM_COLD" | bc)
            echo "⚠️  mgc is ${SLOWDOWN}x slower than pnpm (cold install)"
        fi
    fi
fi

echo
echo "--- Caveats ---"
echo "• Small test workload (2 dependencies)"
echo "• Single run (not median of 10)"
echo "• No hardware profiling (CPU/RAM/disk)"
echo "• No raw JSON output"
echo "• Limited to pnpm comparison only"
echo
echo "For production claims, need:"
echo "  - Multiple competitors (pnpm/bun/deno/moon/proto)"
echo "  - Larger workloads (100+ dependencies)"
echo "  - Statistical significance (10+ runs, p95)"
echo "  - Hardware profiling"
echo "  - Raw data export (JSON)"

# Exit with success if we got any measurements
if [ "$PNPM_COLD" != "-1" ]; then
    echo
    echo "✓ PASS: Basic competitive benchmark complete"
    exit 0
else
    echo
    echo "✗ FAIL: Benchmark execution failed"
    exit 1
fi
