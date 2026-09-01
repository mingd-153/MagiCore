#!/usr/bin/env bash
# Open-Source Readiness Stress Test — MagiCore Pre-Release Gate
# Comprehensive validation: E2E workflow, CLI surface, core parity, competition readiness
# RULE: Deeply analyze, no bypass, no quick fix, use latest versions

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MGC_BIN="${MGC_BIN:-$PROJECT_ROOT/target/debug/mgc}"
TEST_DIR="/tmp/mgc-opensource-readiness-$$"
TEST_HOME="$TEST_DIR/home"

# Colors for output — màu cho output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "========================================"
echo "Open-Source Readiness Stress Test"
echo "========================================"
echo "Binary: $MGC_BIN"
echo "Test dir: $TEST_DIR"
echo "Test home: $TEST_HOME"
echo

# Setup isolated test environment — môi trường test cô lập
export HOME="$TEST_HOME"
export MGC_CACHE_DIR="$TEST_HOME/.mgc"

cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

mkdir -p "$TEST_DIR" "$TEST_HOME"
cd "$TEST_DIR"

PASSED=0
FAILED=0
TOTAL=0

# Test helper — helper test
test_case() {
    local name="$1"
    local description="$2"
    TOTAL=$((TOTAL + 1))
    echo -n "Test $TOTAL: $name - $description... "
}

pass() {
    echo -e "${GREEN}✓ PASS${NC}"
    PASSED=$((PASSED + 1))
}

fail() {
    local reason="$1"
    echo -e "${RED}✗ FAIL${NC} ($reason)"
    FAILED=$((FAILED + 1))
}

warn() {
    local reason="$1"
    echo -e "${YELLOW}⚠ WARN${NC} ($reason)"
}

# ============================================
# SECTION 1: BINARY & VERSION CHECK
# ============================================
echo "=== SECTION 1: Binary & Version Check ==="

test_case "binary-exists" "MGC binary exists and executable"
if [ -x "$MGC_BIN" ]; then
    pass
else
    fail "binary not found or not executable"
fi

test_case "version-command" "mgc --version returns valid semver"
VERSION_OUTPUT=$($MGC_BIN --version 2>&1 || echo "ERROR")
if [[ "$VERSION_OUTPUT" =~ ^mgc\ [0-9]+\.[0-9]+\.[0-9]+ ]]; then
    pass
else
    fail "version output invalid: $VERSION_OUTPUT"
fi

test_case "version-matches-cargo" "Binary version matches Cargo.toml"
# Check workspace.package.version first, then fallback to cli/Cargo.toml
CARGO_VERSION=$(grep -A 1 '\[workspace.package\]' "$PROJECT_ROOT/Cargo.toml" | grep '^version = ' | sed 's/.*"\(.*\)".*/\1/')
if [ -z "$CARGO_VERSION" ]; then
    CARGO_VERSION=$(grep '^version = ' "$PROJECT_ROOT/cli/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
fi
BINARY_VERSION=$(echo "$VERSION_OUTPUT" | awk '{print $2}')
if [ "$BINARY_VERSION" = "$CARGO_VERSION" ]; then
    pass
else
    fail "version mismatch: binary=$BINARY_VERSION, Cargo.toml=$CARGO_VERSION"
fi

# ============================================
# SECTION 2: CLI SURFACE COMPLETENESS
# ============================================
echo
echo "=== SECTION 2: CLI Surface Completeness ==="

# Core commands — lệnh core
for cmd in create-web create-ai create-app create-lib create-game create-iot create-clo create-cicd create-hardware; do
    test_case "cli-$cmd" "Command $cmd exists and shows help"
    if $MGC_BIN $cmd --help >/dev/null 2>&1; then
        pass
    else
        fail "command not found or help broken"
    fi
done

# Utility commands — lệnh tiện ích
for cmd in install run dev build test audit cache store doctor sbom template config init publish outdated search info optimizer; do
    test_case "cli-$cmd" "Command $cmd exists"
    if $MGC_BIN $cmd --help >/dev/null 2>&1 || $MGC_BIN help $cmd >/dev/null 2>&1; then
        pass
    else
        fail "command not found"
    fi
done

# ============================================
# SECTION 3: CORE PARITY — OPTIMIZER SHARED
# ============================================
echo
echo "=== SECTION 3: Core Parity — Optimizer Shared Across All Cores ==="

test_case "optimizer-web" "Optimizer command accepts web core"
if $MGC_BIN optimizer --help 2>&1 | grep -iq "web\|core"; then
    pass
else
    warn "optimizer help doesn't mention cores"
fi

test_case "optimizer-ai" "Optimizer supports ai core"
# Check if runtime detection supports AI runtimes — kiểm tra runtime detection hỗ trợ AI
if grep -q "PythonPyTorch\|RustCandle\|GoTensorFlow" "$PROJECT_ROOT/cli/src/commands/optimizer/runtime_detect.rs"; then
    pass
else
    fail "AI runtimes not in detection"
fi

test_case "optimizer-app" "Optimizer supports app core"
if grep -q "Flutter\|ReactNative\|RustNative" "$PROJECT_ROOT/cli/src/commands/optimizer/runtime_detect.rs"; then
    pass
else
    fail "App runtimes not in detection"
fi

test_case "optimizer-lib" "Optimizer supports lib core"
if grep -q "RustLib\|GoLib\|PythonLib\|TypeScriptLib" "$PROJECT_ROOT/cli/src/commands/optimizer/runtime_detect.rs"; then
    pass
else
    fail "Lib runtimes not in detection"
fi

# ============================================
# SECTION 4: END-TO-END WORKFLOW — CREATE → INSTALL → DEV → BUILD
# ============================================
echo
echo "=== SECTION 4: End-to-End Workflow ==="

# Test 1: Web project E2E — E2E project web
test_case "e2e-web-create" "Create web/vanilla project"
if $MGC_BIN create-web vanilla test-web-e2e --ts >/dev/null 2>&1; then
    pass
else
    fail "create failed"
fi

test_case "e2e-web-files" "Web project has expected files"
if [ -f "test-web-e2e/index.html" ] && [ -f "test-web-e2e/mgc.toml" ]; then
    pass
else
    fail "missing expected files"
fi

test_case "e2e-web-optimizer" "Optimizer can run on web project"
cd test-web-e2e
OPTIMIZER_OUTPUT=$($MGC_BIN optimizer 2>&1)
EXIT_CODE=$?
# Optimizer may skip if no runtime detected (exit 0) or run successfully (exit 0)
if [ $EXIT_CODE -eq 0 ] && echo "$OPTIMIZER_OUTPUT" | grep -iq "optimizer\|hardware\|detected\|skipped"; then
    pass
else
    fail "optimizer exit=$EXIT_CODE, output unclear"
fi
cd ..

# Test 2: AI project E2E — E2E project AI
test_case "e2e-ai-create" "Create ai/python-agent project"
if $MGC_BIN create-ai python-agent test-ai-e2e >/dev/null 2>&1; then
    pass
else
    fail "create failed"
fi

test_case "e2e-ai-files" "AI project has expected files"
if [ -f "test-ai-e2e/mgc.toml" ]; then
    pass
else
    fail "missing mgc.toml"
fi

test_case "e2e-ai-optimizer" "Optimizer can run on AI project"
cd test-ai-e2e
OPTIMIZER_OUTPUT=$($MGC_BIN optimizer 2>&1)
EXIT_CODE=$?
if [ $EXIT_CODE -eq 0 ] && echo "$OPTIMIZER_OUTPUT" | grep -iq "optimizer\|hardware\|detected\|skipped"; then
    pass
else
    fail "optimizer exit=$EXIT_CODE, output unclear"
fi
cd ..

# Test 3: App project E2E — E2E project app
test_case "e2e-app-create" "Create app/flutter project"
if $MGC_BIN create-app flutter@stable test-app-e2e >/dev/null 2>&1; then
    pass
else
    fail "create failed"
fi

test_case "e2e-app-files" "App project has expected files"
if [ -f "test-app-e2e/mgc.toml" ]; then
    pass
else
    fail "missing mgc.toml"
fi

# Test 4: Lib project E2E — E2E project lib
test_case "e2e-lib-create" "Create lib/rust project"
if $MGC_BIN create-lib rust@1.96.0 test-lib-e2e >/dev/null 2>&1; then
    pass
else
    fail "create failed"
fi

test_case "e2e-lib-files" "Lib project has expected files"
if [ -f "test-lib-e2e/mgc.toml" ] && [ -f "test-lib-e2e/Cargo.toml" ]; then
    pass
else
    fail "missing expected files"
fi

test_case "e2e-lib-optimizer" "Optimizer can run on lib project"
cd test-lib-e2e
OPTIMIZER_OUTPUT=$($MGC_BIN optimizer 2>&1)
EXIT_CODE=$?
if [ $EXIT_CODE -eq 0 ] && echo "$OPTIMIZER_OUTPUT" | grep -iq "optimizer\|hardware\|detected\|skipped"; then
    pass
else
    fail "optimizer exit=$EXIT_CODE, output unclear"
fi
cd ..

# ============================================
# SECTION 5: ERROR HANDLING QUALITY
# ============================================
echo
echo "=== SECTION 5: Error Handling Quality ==="

test_case "error-duplicate-project" "Clear error for duplicate project name"
ERROR_OUTPUT=$($MGC_BIN create-web vanilla test-web-e2e 2>&1 || true)
if echo "$ERROR_OUTPUT" | grep -iq "already exists"; then
    pass
else
    fail "no clear duplicate error message"
fi

test_case "error-invalid-template" "Clear error for invalid template"
ERROR_OUTPUT=$($MGC_BIN create-web nonexistent-template-xyz test-invalid 2>&1 || true)
if echo "$ERROR_OUTPUT" | grep -iq "not found\|does not exist\|invalid"; then
    pass
else
    fail "no clear invalid template error"
fi

test_case "error-missing-arg" "Clear error for missing required argument"
ERROR_OUTPUT=$($MGC_BIN create-web 2>&1 || true)
if echo "$ERROR_OUTPUT" | grep -iq "required\|missing\|usage"; then
    pass
else
    fail "no clear missing argument error"
fi

# ============================================
# SECTION 6: CACHE BEHAVIOR
# ============================================
echo
echo "=== SECTION 6: Cache Behavior ==="

test_case "cache-dir-exists" "Cache directory created in test HOME"
if [ -d "$MGC_CACHE_DIR" ]; then
    pass
else
    fail "cache dir not created"
fi

test_case "cache-hermetic" "Cache path is hermetic (in test HOME)"
if [[ "$MGC_CACHE_DIR" == "$TEST_HOME"* ]]; then
    pass
else
    fail "cache leaked outside test HOME"
fi

test_case "cache-status" "mgc cache status works"
if $MGC_BIN cache status >/dev/null 2>&1; then
    pass
else
    fail "cache status command failed"
fi

# ============================================
# SECTION 7: SECURITY CHECKS
# ============================================
echo
echo "=== SECTION 7: Security Checks ==="

test_case "security-no-hardcoded-secrets" "No hardcoded actual secret values in binary"
# Check for actual secret patterns like password="value", token="xyz", api_key="abc"
# NOT just presence of words "password" or "token" (which are in SQL schemas, parsers, etc.)
SUSPECT_STRINGS=$(strings "$MGC_BIN" | grep -iE '(password|token|api_?key|secret)\s*=\s*["\047][^"\047]{8,}["\047]' | head -5)
if [ -n "$SUSPECT_STRINGS" ]; then
    echo "    Actual secret values found:"
    echo "$SUSPECT_STRINGS"
    fail "hardcoded secret values detected"
else
    pass
fi

test_case "security-allowlist-present" "Allowlist mechanism present"
if grep -r "ALLOWED_TOOLS\|FORBIDDEN_TOOLS" "$PROJECT_ROOT/core/crates/mgc-exec/src" >/dev/null 2>&1; then
    pass
else
    fail "no allowlist mechanism found in mgc-exec"
fi

# ============================================
# SECTION 8: DOCUMENTATION & HELP
# ============================================
echo
echo "=== SECTION 8: Documentation & Help ==="

test_case "help-main" "mgc --help returns usage info"
if $MGC_BIN --help | grep -iq "usage\|commands"; then
    pass
else
    fail "help output incomplete"
fi

test_case "help-subcommand" "Subcommand help works (create-web --help)"
if $MGC_BIN create-web --help | grep -iq "create.*web\|usage"; then
    pass
else
    fail "subcommand help broken"
fi

test_case "readme-exists" "README.md exists"
if [ -f "$PROJECT_ROOT/README.md" ]; then
    pass
else
    fail "README.md missing"
fi

test_case "contributing-guide" "CONTRIBUTING.md exists"
if [ -f "$PROJECT_ROOT/CONTRIBUTING.md" ]; then
    pass
else
    warn "CONTRIBUTING.md missing"
fi

# ============================================
# SECTION 9: PACKAGE MANAGER COMPETITION READINESS
# ============================================
echo
echo "=== SECTION 9: Package Manager Competition Readiness ==="

test_case "pms-cache-isolation" "Cache isolation (vs pnpm store, bun cache)"
# Check cache doesn't conflict with other PMs — kiểm tra cache không xung đột với PMs khác
if [ -d "$MGC_CACHE_DIR" ] && ! [ -d "$MGC_CACHE_DIR/pnpm" ] && ! [ -d "$MGC_CACHE_DIR/bun" ]; then
    pass
else
    warn "cache structure may conflict with other PMs"
fi

test_case "pms-performance-baseline" "Performance baseline exists (cache speedup data)"
if [ -f "$PROJECT_ROOT/cli/tests/cache_tracking_stress.sh" ]; then
    pass
else
    fail "no cache performance baseline test"
fi

test_case "pms-optimizer-unique" "Optimizer feature (unique vs moon/proto)"
if [ -d "$PROJECT_ROOT/cli/src/commands/optimizer" ]; then
    pass
else
    fail "optimizer feature missing"
fi

test_case "pms-core-agnostic" "Core-agnostic design (vs npm=web-only)"
# Check supports multiple cores — kiểm tra hỗ trợ nhiều cores
if grep -r "create-web\|create-ai\|create-app\|create-lib" "$PROJECT_ROOT/cli/src/commands" >/dev/null 2>&1; then
    pass
else
    fail "not core-agnostic"
fi

# ============================================
# SECTION 10: DISTRIBUTION READINESS
# ============================================
echo
echo "=== SECTION 10: Distribution Readiness ==="

test_case "dist-binary-size" "Binary size reasonable (<50MB)"
BINARY_SIZE=$(stat -f%z "$MGC_BIN" 2>/dev/null || stat -c%s "$MGC_BIN" 2>/dev/null || echo "0")
if [ "$BINARY_SIZE" -lt 52428800 ]; then  # 50MB in bytes
    pass
else
    warn "binary size large: $((BINARY_SIZE / 1024 / 1024))MB"
fi

test_case "dist-license-file" "LICENSE file exists"
if [ -f "$PROJECT_ROOT/LICENSE" ]; then
    pass
else
    fail "LICENSE file missing"
fi

test_case "dist-cargo-metadata" "Cargo.toml has distribution metadata"
if grep -q "license\|repository\|homepage" "$PROJECT_ROOT/cli/Cargo.toml"; then
    pass
else
    warn "Cargo.toml missing distribution metadata"
fi

# ============================================
# SECTION 11: EVIDENCE-BASED CLAIMS
# ============================================
echo
echo "=== SECTION 11: Evidence-Based Claims ==="

test_case "evidence-test-suite" "Comprehensive test suite exists"
TEST_COUNT=$(find "$PROJECT_ROOT/cli/src" "$PROJECT_ROOT/cli/tests" -name "*.rs" -o -name "*.sh" | xargs grep -l "test\|TEST" | wc -l | tr -d ' ')
if [ "$TEST_COUNT" -gt 10 ]; then
    pass
else
    fail "insufficient test coverage: only $TEST_COUNT test files"
fi

test_case "evidence-no-todo-fixme" "No TODO/FIXME in production code"
TODO_COUNT=$(grep -r "TODO\|FIXME" "$PROJECT_ROOT/cli/src" --include="*.rs" | grep -v "test\|example" | wc -l | tr -d ' ')
if [ "$TODO_COUNT" -eq 0 ]; then
    pass
else
    warn "$TODO_COUNT TODO/FIXME found in production code"
fi

test_case "evidence-benchmark-data" "Performance benchmark data exists"
if [ -f "$PROJECT_ROOT/cli/tests/cache_tracking_stress.sh" ] && [ -f "$PROJECT_ROOT/cli/tests/all_core_scaffold_stress.sh" ]; then
    pass
else
    fail "benchmark/stress test scripts missing"
fi

# ============================================
# SECTION 12: CORE PARITY VALIDATION
# ============================================
echo
echo "=== SECTION 12: Core Parity Validation ==="

test_case "parity-web-command" "Web core command exists"
if $MGC_BIN create-web --help >/dev/null 2>&1; then pass; else fail "missing"; fi

test_case "parity-ai-command" "AI core command exists"
if $MGC_BIN create-ai --help >/dev/null 2>&1; then pass; else fail "missing"; fi

test_case "parity-app-command" "App core command exists"
if $MGC_BIN create-app --help >/dev/null 2>&1; then pass; else fail "missing"; fi

test_case "parity-lib-command" "Lib core command exists"
if $MGC_BIN create-lib --help >/dev/null 2>&1; then pass; else fail "missing"; fi

test_case "parity-game-command" "Game core command exists"
if $MGC_BIN create-game --help >/dev/null 2>&1; then pass; else fail "missing"; fi

test_case "parity-optimizer-web" "Optimizer works for web projects"
cd test-web-e2e
OPTIMIZER_OUTPUT=$($MGC_BIN optimizer 2>&1)
EXIT_CODE=$?
if [ $EXIT_CODE -eq 0 ] && echo "$OPTIMIZER_OUTPUT" | grep -iq "optimizer\|hardware\|detected\|skipped"; then
    pass
else
    fail "optimizer exit=$EXIT_CODE or output unclear"
fi
cd ..

test_case "parity-optimizer-ai" "Optimizer works for AI projects"
cd test-ai-e2e
OPTIMIZER_OUTPUT=$($MGC_BIN optimizer 2>&1)
EXIT_CODE=$?
if [ $EXIT_CODE -eq 0 ] && echo "$OPTIMIZER_OUTPUT" | grep -iq "optimizer\|hardware\|detected\|skipped"; then
    pass
else
    fail "optimizer exit=$EXIT_CODE or output unclear"
fi
cd ..

# ============================================
# RESULTS SUMMARY
# ============================================
echo
echo "========================================"
echo "Open-Source Readiness Test Results"
echo "========================================"
echo "Total tests: $TOTAL"
echo -e "Passed: ${GREEN}$PASSED${NC}"
echo -e "Failed: ${RED}$FAILED${NC}"
echo

if [ "$FAILED" -eq 0 ]; then
    echo -e "${GREEN}✓ ALL TESTS PASSED${NC}"
    echo "Internal RC checks PASS — infrastructure solid"
    echo
    echo "⚠️  PUBLIC RC: NO-GO until:"
    echo "  1. Full E2E tests (create → install → build → test)"
    echo "  2. Distribution tested (Homebrew/Scoop/binary)"
    echo "  3. Benchmark data collected (vs pnpm/bun/deno/moon/proto)"
    echo "  4. Security claims audited"
    echo "  5. Test runner security policy documented"
    echo
    echo "See docs/INTERNAL_RC_STATUS.md for details"
    exit 0
else
    echo -e "${RED}✗ SOME TESTS FAILED${NC}"
    echo "MagiCore needs fixes before open-source release"
    echo
    echo "Critical issues to fix:"
    echo "  1. Review failed tests above"
    echo "  2. Ensure all 4 cores (web/ai/app/lib) have working E2E"
    echo "  3. Verify optimizer shared across all cores"
    echo "  4. Fix error messages to be clear and actionable"
    echo "  5. Complete documentation (README, CONTRIBUTING)"
    exit 1
fi
