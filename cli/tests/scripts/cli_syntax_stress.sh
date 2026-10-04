#!/usr/bin/env bash
# CLI Syntax Stress Test — Phase 1 All-Core Stress Test Pack
# Test full command + alias coverage with temp HOME hermetic

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
MGC_BIN="${MGC_BIN:-$PROJECT_ROOT/target/debug/mgc}"
TEST_DIR="/tmp/mgc-cli-stress-$$"
TEST_HOME="$TEST_DIR/home"

# Setup isolated test environment — môi trường test cô lập
export HOME="$TEST_HOME"
export MGC_CACHE_DIR="$TEST_HOME/.mgc"

PASSED=0
TOTAL=0

echo "=== CLI Syntax Stress Test ==="
echo "Binary: $MGC_BIN"
echo "Test dir: $TEST_DIR"
echo "Test home: $TEST_HOME"
echo

# Cleanup — dọn dẹp
cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

mkdir -p "$TEST_DIR" "$TEST_HOME"
cd "$TEST_DIR"

run_test() {
    local name="$1"
    shift
    TOTAL=$((TOTAL + 1))
    echo "Test $TOTAL: $name"
    if "$@" >/dev/null 2>&1; then
        echo "✓ PASS"
        PASSED=$((PASSED + 1))
        return 0
    else
        echo "✗ FAIL"
        return 1
    fi
}

run_test_expect_output() {
    local name="$1"
    local expected="$2"
    shift 2
    TOTAL=$((TOTAL + 1))
    echo "Test $TOTAL: $name"
    local output
    output=$("$@" 2>&1 || true)
    if echo "$output" | grep -q "$expected"; then
        echo "✓ PASS (found: '$expected')"
        PASSED=$((PASSED + 1))
        return 0
    else
        echo "✗ FAIL (expected '$expected' not found)"
        echo "   Output: ${output:0:200}"
        return 1
    fi
}

# === GLOBAL COMMANDS === — lệnh toàn cục
run_test "mgc --version" "$MGC_BIN" --version
run_test "mgc --help" "$MGC_BIN" --help

# === CREATE COMMANDS (full + alias) === — lệnh tạo (đầy đủ + alias)
run_test_expect_output "create-web (full)" "FRAMEWORK" "$MGC_BIN" create-web --help
run_test_expect_output "cre-w (alias)" "FRAMEWORK" "$MGC_BIN" cre-w --help

run_test_expect_output "create-ai (full)" "FRAMEWORK" "$MGC_BIN" create-ai --help
run_test_expect_output "cre-ai (alias)" "FRAMEWORK" "$MGC_BIN" cre-ai --help

run_test_expect_output "create-app (full)" "FRAMEWORK" "$MGC_BIN" create-app --help
run_test_expect_output "cre-a (alias)" "FRAMEWORK" "$MGC_BIN" cre-a --help

run_test_expect_output "create-lib (full)" "FRAMEWORK" "$MGC_BIN" create-lib --help
run_test_expect_output "cre-l (alias)" "FRAMEWORK" "$MGC_BIN" cre-l --help

run_test_expect_output "create-game (full)" "FRAMEWORK" "$MGC_BIN" create-game --help
run_test_expect_output "cre-g (alias)" "FRAMEWORK" "$MGC_BIN" cre-g --help

run_test_expect_output "create-clo (full)" "FRAMEWORK" "$MGC_BIN" create-clo --help
run_test_expect_output "cre-c (alias)" "FRAMEWORK" "$MGC_BIN" cre-c --help

run_test_expect_output "create-cicd (full)" "FRAMEWORK" "$MGC_BIN" create-cicd --help
run_test_expect_output "cre-ci (alias)" "FRAMEWORK" "$MGC_BIN" cre-ci --help

run_test_expect_output "create-iot (full)" "FRAMEWORK" "$MGC_BIN" create-iot --help
run_test_expect_output "cre-i (alias)" "FRAMEWORK" "$MGC_BIN" cre-i --help

run_test_expect_output "create-hardware (full)" "FRAMEWORK" "$MGC_BIN" create-hardware --help
run_test_expect_output "cre-h (alias)" "FRAMEWORK" "$MGC_BIN" cre-h --help

# === INSTALL COMMANDS (full + alias) === — lệnh cài đặt (đầy đủ + alias)
run_test_expect_output "install (full)" "Install" "$MGC_BIN" install --help
run_test_expect_output "i (alias)" "Install" "$MGC_BIN" i --help

run_test_expect_output "install-web (full)" "Install web" "$MGC_BIN" install-web --help
run_test_expect_output "i-web (alias)" "Install web" "$MGC_BIN" i-web --help

run_test_expect_output "install-game (full)" "Install game" "$MGC_BIN" install-game --help
run_test_expect_output "i-game (alias)" "Install game" "$MGC_BIN" i-game --help

run_test_expect_output "install-ai (full)" "Install AI" "$MGC_BIN" install-ai --help
run_test_expect_output "i-ai (alias)" "Install AI" "$MGC_BIN" i-ai --help

run_test_expect_output "install-clo (full)" "Install cloud" "$MGC_BIN" install-clo --help
run_test_expect_output "i-clo (alias)" "Install cloud" "$MGC_BIN" i-clo --help

run_test_expect_output "install-cicd (full)" "Install CI/CD" "$MGC_BIN" install-cicd --help
run_test_expect_output "i-cicd (alias)" "Install CI/CD" "$MGC_BIN" i-cicd --help

run_test_expect_output "install-iot (full)" "Install IoT" "$MGC_BIN" install-iot --help
run_test_expect_output "i-iot (alias)" "Install IoT" "$MGC_BIN" i-iot --help

run_test_expect_output "install-app (full)" "Install app" "$MGC_BIN" install-app --help
run_test_expect_output "i-app (alias)" "Install app" "$MGC_BIN" i-app --help

run_test_expect_output "install-lib (full)" "Install lib" "$MGC_BIN" install-lib --help
run_test_expect_output "i-lib (alias)" "Install lib" "$MGC_BIN" i-lib --help

run_test_expect_output "install-hardware (full)" "Install hardware" "$MGC_BIN" install-hardware --help
run_test_expect_output "i-hardware (alias)" "Install hardware" "$MGC_BIN" i-hardware --help

# === ADD COMMANDS === — lệnh thêm package
run_test_expect_output "add-web (full)" "Add web" "$MGC_BIN" add-web --help
run_test_expect_output "add-game (full)" "Add game" "$MGC_BIN" add-game --help
run_test_expect_output "add-ai (full)" "Add AI" "$MGC_BIN" add-ai --help
run_test_expect_output "add-lib (full)" "Add lib" "$MGC_BIN" add-lib --help

# === BUILD/DEV COMMANDS === — lệnh build/dev
run_test_expect_output "dev (full)" "dev" "$MGC_BIN" dev --help
run_test_expect_output "build (full)" "build" "$MGC_BIN" build --help

# === AUDIT/SECURITY === — kiểm tra bảo mật
run_test_expect_output "audit (full)" "Audit" "$MGC_BIN" audit --help

# === CACHE === — quản lý cache
run_test_expect_output "cache status" "status" "$MGC_BIN" cache status --help
run_test_expect_output "cache clean" "clean" "$MGC_BIN" cache clean --help

# === STORE === — quản lý store
run_test_expect_output "store status" "status" "$MGC_BIN" store status --help
run_test_expect_output "store prune" "prune" "$MGC_BIN" store prune --help

# === DOCTOR === — chẩn đoán
run_test_expect_output "doctor (full)" "doctor" "$MGC_BIN" doctor --help

# === SBOM === — Software Bill of Materials
run_test_expect_output "sbom (full)" "SBOM" "$MGC_BIN" sbom --help

# === TEMPLATE === — quản lý template
run_test_expect_output "template list" "list" "$MGC_BIN" template list --help
run_test_expect_output "template fetch" "fetch" "$MGC_BIN" template fetch --help

# === CONFIG === — cấu hình
run_test_expect_output "config (full)" "configuration" "$MGC_BIN" config --help
run_test_expect_output "c (alias)" "configuration" "$MGC_BIN" c --help

# === INIT === — khởi tạo project
run_test_expect_output "init (full)" "Interactive project wizard" "$MGC_BIN" init --help

# === PUBLISH === — xuất bản
run_test_expect_output "publish (full)" "Publish" "$MGC_BIN" publish --help

# === OUTDATED === — kiểm tra package cũ
run_test_expect_output "outdated (full)" "outdated" "$MGC_BIN" outdated --help

# === SEARCH === — tìm kiếm package
run_test_expect_output "search (full)" "Search" "$MGC_BIN" search --help

# === INFO === — thông tin package
run_test_expect_output "info (full)" "package information" "$MGC_BIN" info --help

echo
echo "=== Results ==="
echo "Passed: $PASSED/$TOTAL"

if [ "$PASSED" -eq "$TOTAL" ]; then
    echo "✓ ALL CLI SYNTAX TESTS PASSED"
    exit 0
else
    FAILED=$((TOTAL - PASSED))
    echo "✗ $FAILED CLI SYNTAX TESTS FAILED"
    exit 1
fi
