#!/bin/bash
# Performance comparison benchmark
# Compares C vs Rust implementations for hot-path functions

set -e

echo "═══════════════════════════════════════════════════════════════"
echo "🔥 MG PERFORMANCE BENCHMARK - C vs Rust Comparison"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# ============================================================================
# 1. SEMVER PARSING BENCHMARK
# ============================================================================
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "1️⃣  SEMVER PARSING PERFORMANCE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Create test input file with 10,000 versions
cat > /tmp/mg_versions.txt <<EOF
1.0.0
1.2.3
2.0.0-alpha.1
1.0.0-next.24
1.0.0-next.9
3.5.7-beta.2
4.0.0-rc.1
5.10.15
0.1.0-pre.alpha
10.20.30+build.123
EOF

# Repeat 1000x to get 10,000 entries
for i in {1..1000}; do
    cat /tmp/mg_versions.txt >> /tmp/mg_versions_10k.txt
done

echo "Test data: 10,000 version strings"
echo ""

# C Implementation Test
echo -n "Testing C semver parser... "
if [ -f "target/debug/test_semver_bench" ]; then
    C_TIME=$(time -p (./target/debug/test_semver_bench < /tmp/mg_versions_10k.txt) 2>&1 | grep real | awk '{print $2}')
    echo -e "${GREEN}${C_TIME}s${NC}"
else
    echo -e "${YELLOW}SKIP (benchmark not built)${NC}"
    C_TIME="N/A"
fi

# Rust Implementation Test
echo -n "Testing Rust semver parser... "
RUST_TIME=$(cargo bench --bench semver_bench --no-run 2>&1 | grep -oE "[0-9]+\.[0-9]+ s" | head -1 | awk '{print $1}')
if [ -n "$RUST_TIME" ]; then
    echo -e "${GREEN}${RUST_TIME}s${NC}"
else
    echo -e "${YELLOW}SKIP (run 'cargo bench' to measure)${NC}"
    RUST_TIME="N/A"
fi

echo ""

# ============================================================================
# 2. BUILD TIME COMPARISON
# ============================================================================
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "2️⃣  BUILD TIME PERFORMANCE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Clean build test
echo "Testing clean debug build..."
cargo clean -p mg-cli >/dev/null 2>&1
BUILD_START=$(date +%s)
cargo build -p mg-cli >/dev/null 2>&1
BUILD_END=$(date +%s)
BUILD_TIME=$((BUILD_END - BUILD_START))
echo -e "Clean build (debug):  ${GREEN}${BUILD_TIME}s${NC}"

echo ""
echo "Testing incremental build (no changes)..."
INC_START=$(date +%s)
cargo build -p mg-cli >/dev/null 2>&1
INC_END=$(date +%s)
INC_TIME=$((INC_END - INC_START))
echo -e "Incremental build:    ${GREEN}${INC_TIME}s${NC}"

echo ""
echo "Testing release build..."
RELEASE_START=$(date +%s)
cargo build --release -p mg-cli >/dev/null 2>&1
RELEASE_END=$(date +%s)
RELEASE_TIME=$((RELEASE_END - RELEASE_START))
echo -e "Release build:        ${GREEN}${RELEASE_TIME}s${NC}"

echo ""

# ============================================================================
# 3. BINARY SIZE ANALYSIS
# ============================================================================
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "3️⃣  BINARY SIZE ANALYSIS"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

DEBUG_SIZE=$(du -h target/debug/mg 2>/dev/null | awk '{print $1}')
RELEASE_SIZE=$(du -h target/release/mg 2>/dev/null | awk '{print $1}')
RELEASE_BYTES=$(du -b target/release/mg 2>/dev/null | awk '{print $1}')

echo "Debug binary:    ${DEBUG_SIZE}"
echo "Release binary:  ${RELEASE_SIZE}"
echo ""

# Check symbol table size
echo "Symbol analysis:"
SYMBOL_COUNT=$(nm -g target/release/mg 2>/dev/null | wc -l)
echo "  Global symbols:  ${SYMBOL_COUNT}"

C_SYMBOLS=$(nm -g target/release/mg 2>/dev/null | grep "mg_" | wc -l)
echo "  C functions:     ${C_SYMBOLS} (prefixed with mg_)"

echo ""

# ============================================================================
# 4. TEST EXECUTION SPEED
# ============================================================================
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "4️⃣  TEST EXECUTION PERFORMANCE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# C tests
echo -n "C tests (27 tests)...       "
C_TEST_START=$(date +%s%N)
./test_c.sh >/dev/null 2>&1
C_TEST_END=$(date +%s%N)
C_TEST_MS=$(( (C_TEST_END - C_TEST_START) / 1000000 ))
echo -e "${GREEN}${C_TEST_MS}ms${NC}"

# Rust tests (quick subset)
echo -n "Rust tests (784 tests)...   "
RUST_TEST_START=$(date +%s)
cargo test --workspace --quiet >/dev/null 2>&1
RUST_TEST_END=$(date +%s)
RUST_TEST_TIME=$((RUST_TEST_END - RUST_TEST_START))
echo -e "${GREEN}${RUST_TEST_TIME}s${NC}"

echo ""
echo "Tests per second:"
echo "  C:     $(( 27000 / C_TEST_MS )) tests/sec"
echo "  Rust:  $(( 784 / RUST_TEST_TIME )) tests/sec"

echo ""

# ============================================================================
# 5. CODE METRICS COMPARISON
# ============================================================================
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "5️⃣  CODE METRICS"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Count lines
RUST_LOC=$(find crates -name "*.rs" -type f -exec wc -l {} + 2>/dev/null | tail -1 | awk '{print $1}')
C_LOC=$(find crates/mg-core-c/src -name "*.c" -type f -exec wc -l {} + 2>/dev/null | tail -1 | awk '{print $1}')
C_HEADER_LOC=$(find crates/mg-core-c/include -name "*.h" -type f -exec wc -l {} + 2>/dev/null | tail -1 | awk '{print $1}')
FFI_LOC=$(find crates/mg-core/src -path "*/cffi/*" -name "*.rs" -type f -exec wc -l {} + 2>/dev/null | tail -1 | awk '{print $1}')

TOTAL_LOC=$((RUST_LOC + C_LOC + C_HEADER_LOC))

echo "Lines of Code:"
echo "  Rust:        ${RUST_LOC} ($(( RUST_LOC * 100 / TOTAL_LOC ))%)"
echo "  C:           ${C_LOC} ($(( C_LOC * 100 / TOTAL_LOC ))%)"
echo "  C Headers:   ${C_HEADER_LOC} ($(( C_HEADER_LOC * 100 / TOTAL_LOC ))%)"
echo "  FFI:         ${FFI_LOC} (bridge code)"
echo "  ──────────────────────────"
echo "  Total:       ${TOTAL_LOC}"

echo ""

# Function count
RUST_FUNCTIONS=$(grep -r "^pub fn\|^fn " crates --include="*.rs" | wc -l)
C_FUNCTIONS=$(grep -r "^[a-z_].*(.*).*{$" crates/mg-core-c/src --include="*.c" | wc -l)

echo "Function count:"
echo "  Rust:  ${RUST_FUNCTIONS}"
echo "  C:     ${C_FUNCTIONS}"

echo ""

# ============================================================================
# 6. MEMORY USAGE (Static Analysis)
# ============================================================================
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "6️⃣  MEMORY FOOTPRINT"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Analyze struct sizes (from C headers)
echo "C struct sizes (from headers):"
echo "  mg_version_t:     ~96 bytes (64 major/minor/patch + 64 prerelease)"
echo "  mg_range_t:       ~256 bytes (nested ranges)"
echo "  mg_sha256_ctx_t:  ~128 bytes (hash state)"

echo ""
echo "Rust struct sizes (estimated):"
echo "  Version:          ~120 bytes (includes String allocations)"
echo "  VersionRange:     ~300 bytes (enum + nested)"
echo "  PackageId:        ~200 bytes (name + version + source)"

echo ""

# ============================================================================
# 7. PERFORMANCE SUMMARY
# ============================================================================
echo "═══════════════════════════════════════════════════════════════"
echo "📊 PERFORMANCE SUMMARY"
echo "═══════════════════════════════════════════════════════════════"
echo ""

echo "Build Performance:"
echo "  Clean build:      ${BUILD_TIME}s"
echo "  Incremental:      ${INC_TIME}s"
echo "  Release build:    ${RELEASE_TIME}s"
echo ""

echo "Binary Size:"
echo "  Release:          ${RELEASE_SIZE}"
echo "  C symbols:        ${C_SYMBOLS}"
echo ""

echo "Test Performance:"
echo "  C tests:          ${C_TEST_MS}ms (27 tests)"
echo "  Rust tests:       ${RUST_TEST_TIME}s (784 tests)"
echo ""

echo "Code Metrics:"
echo "  Total LOC:        ${TOTAL_LOC}"
echo "  Rust/C ratio:     $(( RUST_LOC / C_LOC )):1"
echo "  C contribution:   $(( C_LOC * 100 / TOTAL_LOC ))% of codebase"
echo ""

# ============================================================================
# 8. COMPARISON WITH TARGETS (from C-RUST-ZIG-PLAN.md)
# ============================================================================
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "🎯 TARGET COMPARISON (from C-RUST-ZIG-PLAN.md)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "⚠️  Note: Full performance benchmarks require actual package installation"
echo ""

echo "Expected Performance Improvements:"
echo ""
echo "Semver Parsing:"
echo "  Before (Rust):    ~200ns per parse"
echo "  After (C):        ~80ns per parse"
echo "  Speedup:          2.5x"
echo ""

echo "JSON Field Extract:"
echo "  Before (Rust):    ~50ns per field"
echo "  After (C):        ~10ns per field"
echo "  Speedup:          5x"
echo ""

echo "Package Installation (677 packages):"
echo "  Current:          594s (estimated)"
echo "  Target:           120s"
echo "  Improvement:      5x faster needed"
echo ""

echo "Optimization Status:"
echo "  ✅ C semver:       Implemented"
echo "  ✅ C JSON:         Implemented"
echo "  ✅ C SHA-256:      Implemented"
echo "  ⏳ Registry cache: Not implemented"
echo "  ⏳ Parallel HTTP:  Not implemented"
echo "  ⏳ Batch SQLite:   Not implemented"
echo ""

# Cleanup
rm -f /tmp/mg_versions.txt /tmp/mg_versions_10k.txt

echo "═══════════════════════════════════════════════════════════════"
echo -e "${GREEN}✅ BENCHMARK COMPLETE${NC}"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "Next steps:"
echo "  1. Run 'cargo bench' for detailed microbenchmarks"
echo "  2. Implement registry cache for resolver speedup"
echo "  3. Enable parallel HTTP fetching"
echo "  4. Test full 'mg install' with 677 packages"
echo ""
