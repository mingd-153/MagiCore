#!/bin/bash
# Real-world performance benchmark script
# Tests actual install performance with timing breakdown

set -e

echo "═══════════════════════════════════════════════════════════════"
echo "🔥 MG REAL-WORLD PERFORMANCE BENCHMARK"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Create test directory
TEST_DIR="/tmp/mg-perf-test-$(date +%s)"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

echo "Test directory: $TEST_DIR"
echo ""

# ============================================================================
# 1. SMALL TEST (10 packages)
# ============================================================================
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "1️⃣  SMALL PROJECT TEST (10 packages)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

cat > package.json <<'EOF'
{
  "name": "small-test",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "typescript": "^5.0.0",
    "eslint": "^8.0.0",
    "prettier": "^3.0.0"
  },
  "devDependencies": {
    "@types/react": "^18.0.0",
    "@types/node": "^20.0.0",
    "vite": "^5.0.0",
    "vitest": "^1.0.0",
    "tsx": "^4.0.0"
  }
}
EOF

echo -e "${BLUE}Starting small install...${NC}"
SMALL_START=$(date +%s%N)
"$OLDPWD/target/release/mg" install --offline=false 2>&1 | grep -E "\[TIMING\]|\[PREFETCH\]|\[INFO\]" || true
SMALL_END=$(date +%s%N)
SMALL_MS=$(( (SMALL_END - SMALL_START) / 1000000 ))

echo ""
echo -e "${GREEN}Small install: ${SMALL_MS}ms${NC}"
echo ""

# Count packages
if [ -d "node_modules" ]; then
    SMALL_PKGS=$(find node_modules -maxdepth 2 -name "package.json" | wc -l | tr -d ' ')
    echo "Packages installed: ${SMALL_PKGS}"
fi

# ============================================================================
# 2. MEDIUM TEST (50+ packages)
# ============================================================================
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "2️⃣  MEDIUM PROJECT TEST (50+ packages)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

cd "$TEST_DIR"
rm -rf node_modules mg.lock package.json

cat > package.json <<'EOF'
{
  "name": "medium-test",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "next": "^14.0.0",
    "typescript": "^5.0.0",
    "express": "^4.18.0",
    "axios": "^1.6.0",
    "lodash": "^4.17.0",
    "date-fns": "^3.0.0",
    "zod": "^3.22.0",
    "prisma": "^5.0.0"
  },
  "devDependencies": {
    "@types/react": "^18.0.0",
    "@types/node": "^20.0.0",
    "@types/express": "^4.17.0",
    "eslint": "^8.0.0",
    "eslint-config-next": "^14.0.0",
    "prettier": "^3.0.0",
    "vitest": "^1.0.0",
    "tsx": "^4.0.0",
    "@testing-library/react": "^14.0.0",
    "@testing-library/jest-dom": "^6.0.0"
  }
}
EOF

echo -e "${BLUE}Starting medium install...${NC}"
MEDIUM_START=$(date +%s%N)
"$OLDPWD/target/release/mg" install --offline=false 2>&1 | grep -E "\[TIMING\]|\[PREFETCH\]|\[INFO\]" || true
MEDIUM_END=$(date +%s%N)
MEDIUM_MS=$(( (MEDIUM_END - MEDIUM_START) / 1000000 ))

echo ""
echo -e "${GREEN}Medium install: ${MEDIUM_MS}ms ($(($MEDIUM_MS / 1000))s)${NC}"
echo ""

if [ -d "node_modules" ]; then
    MEDIUM_PKGS=$(find node_modules -maxdepth 2 -name "package.json" | wc -l | tr -d ' ')
    echo "Packages installed: ${MEDIUM_PKGS}"
fi

# ============================================================================
# 3. CACHE HIT TEST (re-install)
# ============================================================================
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "3️⃣  CACHE HIT TEST (re-install same packages)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

rm -rf node_modules mg.lock

echo -e "${BLUE}Starting cached install...${NC}"
CACHE_START=$(date +%s%N)
"$OLDPWD/target/release/mg" install --offline=false 2>&1 | grep -E "\[TIMING\]|\[PREFETCH\]|\[INFO\]" || true
CACHE_END=$(date +%s%N)
CACHE_MS=$(( (CACHE_END - CACHE_START) / 1000000 ))

echo ""
echo -e "${GREEN}Cached install: ${CACHE_MS}ms ($(($CACHE_MS / 1000))s)${NC}"
echo ""

if [ -d "node_modules" ]; then
    CACHE_PKGS=$(find node_modules -maxdepth 2 -name "package.json" | wc -l | tr -d ' ')
    echo "Packages installed: ${CACHE_PKGS}"
fi

# ============================================================================
# 4. SUMMARY
# ============================================================================
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "📊 BENCHMARK SUMMARY"
echo "═══════════════════════════════════════════════════════════════"
echo ""

echo "Test Results:"
echo "  Small project (~10 deps):     ${SMALL_MS}ms (${SMALL_PKGS} packages)"
echo "  Medium project (~50 deps):    ${MEDIUM_MS}ms = $(($MEDIUM_MS / 1000))s (${MEDIUM_PKGS} packages)"
echo "  Cached re-install:            ${CACHE_MS}ms = $(($CACHE_MS / 1000))s (${CACHE_PKGS} packages)"
echo ""

if [ "$MEDIUM_MS" -gt 0 ]; then
    PKG_PER_SEC=$((MEDIUM_PKGS * 1000 / MEDIUM_MS))
    echo "Performance:"
    echo "  Packages per second:          ${PKG_PER_SEC} pkg/s"
    echo "  Avg time per package:         $((MEDIUM_MS / MEDIUM_PKGS))ms"
fi

if [ "$CACHE_MS" -gt 0 ] && [ "$MEDIUM_MS" -gt 0 ]; then
    SPEEDUP=$((MEDIUM_MS / CACHE_MS))
    echo ""
    echo "Cache speedup:                  ${SPEEDUP}x faster"
fi

echo ""
echo "Test directory: $TEST_DIR"
echo "(Cleanup: rm -rf $TEST_DIR)"
echo ""

echo "═══════════════════════════════════════════════════════════════"
echo -e "${GREEN}✅ BENCHMARK COMPLETE${NC}"
echo "═══════════════════════════════════════════════════════════════"
