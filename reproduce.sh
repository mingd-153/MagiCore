#!/bin/bash
# reproduce.sh — Reproduce MagiCore benchmarks
# Usage: curl -fsSL https://raw.githubusercontent.com/mingd-153/MagiCore/main/reproduce.sh | bash
# Or: ./reproduce.sh (if repo already cloned)

set -e

echo "📦 MagiCore Benchmark Reproducibility Script"
echo "============================================="
echo ""
echo "This script will:"
echo "  1. Check prerequisites (mgc, npm, bun)"
echo "  2. Clone MagiCore repo (if not present)"
echo "  3. Run benchmark suite (~30-60 min)"
echo "  4. Compare your results with published baseline"
echo ""

# Prerequisites check
echo "🔍 Checking prerequisites..."
echo ""

command -v mgc >/dev/null 2>&1 || {
  echo "❌ mgc not found."
  echo ""
  echo "Install MagiCore:"
  echo "  macOS:   brew install mingd-153/tap/magicore"
  echo "  Linux:   curl -fsSL https://magicore.dev/install.sh | sh"
  echo "  Windows: scoop install magicore"
  echo ""
  exit 1
}

command -v npm >/dev/null 2>&1 || {
  echo "❌ npm not found."
  echo ""
  echo "Install Node.js: https://nodejs.org"
  echo ""
  exit 1
}

# Optional: bun check (not required, but nice to have)
if ! command -v bun >/dev/null 2>&1; then
  echo "⚠️  bun not found. Bun benchmarks will be skipped."
  echo "   Install: https://bun.sh"
  echo ""
fi

# Optional: pnpm check
if ! command -v pnpm >/dev/null 2>&1; then
  echo "⚠️  pnpm not found. pnpm benchmarks will be skipped."
  echo "   Install: npm install -g pnpm"
  echo ""
fi

echo "✅ Prerequisites OK"
echo ""
echo "Detected versions:"
echo "  mgc:  $(mgc --version 2>&1 | head -1)"
echo "  npm:  $(npm --version)"
[ -x "$(command -v bun)" ] && echo "  bun:  $(bun --version)"
[ -x "$(command -v pnpm)" ] && echo "  pnpm: $(pnpm --version)"
echo ""

# Clone repo (if needed)
if [ -d "MagiCore/benchmark" ]; then
  echo "✅ MagiCore repo already present"
  cd MagiCore
else
  echo "📥 Cloning MagiCore repo..."
  git clone --depth 1 https://github.com/mingd-153/MagiCore.git || {
    echo "❌ Failed to clone repo. Check internet connection."
    exit 1
  }
  cd MagiCore
fi

# Verify benchmark script exists
if [ ! -f "benchmark/scripts/run_benchmark_native.sh" ]; then
  echo "❌ Benchmark script not found. Repo may be corrupted."
  echo "   Try: rm -rf MagiCore && ./reproduce.sh"
  exit 1
fi

cd benchmark

# Run benchmarks
echo ""
echo "🚀 Running benchmarks..."
echo "================================================"
echo ""
echo "⏱️  Estimated time: 30-60 minutes"
echo "   - Cold installs: Download packages from registries"
echo "   - Warm installs: Use local cache"
echo "   - 4 package managers: mgc, npm, bun, pnpm"
echo ""
echo "☕ Grab a coffee while this runs..."
echo ""

# Run with error handling
if ./scripts/run_benchmark_native.sh; then
  echo ""
  echo "✅ Benchmarks completed successfully!"
else
  echo ""
  echo "⚠️  Benchmark script encountered errors."
  echo "   Check logs above for details."
  echo ""
  exit 1
fi

# Parse results
echo ""
echo "📊 Your Results:"
echo "================"
echo ""

for file in results/*.json; do
  [ -e "$file" ] || continue
  PM=$(basename "$file" | cut -d_ -f1)
  COLD=$(jq -r '.cold_install_seconds // "N/A"' "$file" 2>/dev/null || echo "N/A")
  WARM=$(jq -r '.warm_install_seconds // "N/A"' "$file" 2>/dev/null || echo "N/A")
  DISK=$(jq -r '.total_disk_mb // "N/A"' "$file" 2>/dev/null || echo "N/A")
  
  printf "%-10s Cold: %7s  Warm: %6s  Disk: %6s MB\n" "$PM" "${COLD}s" "${WARM}s" "$DISK"
done

echo ""
echo "📄 Published Baseline (M2 macOS, 2026-08-27):"
echo "=============================================="
echo ""
echo "mgc        Cold:  ~TBD     Warm: ~TBD     Disk: ~TBD MB  (testing)"
echo "npm        Cold:  529.8s   Warm:  11.0s   Disk: 562 MB"
echo "bun        Cold:   96.3s   Warm:   0.6s   Disk: 538 MB"
echo "pnpm       Cold:  >120s    Warm:   N/A    Disk: N/A     (timeout)"
echo ""

# Calculate speedup (mgc vs npm)
MGC_TIME=$(jq -r '.cold_install_seconds' results/mgc_*.json 2>/dev/null || echo "0")
NPM_TIME=$(jq -r '.cold_install_seconds' results/npm_*.json 2>/dev/null || echo "0")

if [ "$MGC_TIME" != "0" ] && [ "$MGC_TIME" != "N/A" ] && [ "$NPM_TIME" != "0" ]; then
  # Use awk for float division (bc may not be available)
  RATIO=$(awk "BEGIN {printf \"%.1f\", $NPM_TIME / $MGC_TIME}")
  echo "⚡ Your speedup: mgc ${RATIO}x faster than npm (cold install)"
  echo ""
  
  if (( $(echo "$RATIO < 2.0" | awk '{print ($1 < 2.0)}') )); then
    echo "⚠️  Speedup lower than expected. Possible reasons:"
    echo "   - Network latency (slow registry)"
    echo "   - First-run overhead (OS disk cache)"
    echo "   - System load (background processes)"
    echo "   Run again for more stable results."
  elif (( $(echo "$RATIO > 10.0" | awk '{print ($1 > 10.0)}') )); then
    echo "🎉 Excellent speedup! Your machine benefits from mgc's optimizations."
  else
    echo "✅ Speedup within expected range (2-10x faster than npm)."
  fi
else
  echo "⚠️  Could not calculate speedup. Check if mgc and npm benchmarks completed successfully."
  echo "   Results files: $(ls results/*.json 2>/dev/null | tr '\n' ' ')"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "✅ Reproducibility test complete!"
echo ""
echo "📤 Share your results with the community:"
echo "   https://github.com/mingd-153/MagiCore/discussions/new?category=benchmarks"
echo ""
echo "🐛 Found issues? Report here:"
echo "   https://github.com/mingd-153/MagiCore/issues/new"
echo ""
echo "📖 Full benchmark methodology:"
echo "   https://github.com/mingd-153/MagiCore/blob/main/docs/BENCHMARK.md"
echo ""
echo "Thank you for testing MagiCore! 🚀"
echo ""
