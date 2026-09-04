#!/usr/bin/env bash
# CLI Syntax Stress Test — Phase 1 All-Core Stress Test Pack
# Test full command + alias coverage with temp HOME hermetic

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
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
    local command="$2"
    TOTAL=$((TOTAL + 1))
    echo "Test $TOTAL: $name"
    if eval "$command" >/dev/null 2>&1; then
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
    local command="$2"
    local expected="$3"
    TOTAL=$((TOTAL + 1))
    echo "Test $TOTAL: $name"
    local output
    output=$($command 2>&1 || true)
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
run_test "mgc --version" "$MGC_BIN --version"
run_test "mgc --help" "$MGC_BIN --help"

# === CREATE COMMANDS (full + alias) === — lệnh tạo (đầy đủ + alias)
run_test_expect_output "create-web (full)" "$MGC_BIN create-web --help" "FRAMEWORK"
run_test_expect_output "cre-w (alias)" "$MGC_BIN cre-w --help" "FRAMEWORK"

run_test_expect_output "create-ai (full)" "$MGC_BIN create-ai --help" "FRAMEWORK"
run_test_expect_output "cre-ai (alias)" "$MGC_BIN cre-ai --help" "FRAMEWORK"

run_test_expect_output "create-app (full)" "$MGC_BIN create-app --help" "FRAMEWORK"
run_test_expect_output "cre-a (alias)" "$MGC_BIN cre-a --help" "FRAMEWORK"

run_test_expect_output "create-lib (full)" "$MGC_BIN create-lib --help" "FRAMEWORK"
run_test_expect_output "cre-l (alias)" "$MGC_BIN cre-l --help" "FRAMEWORK"

run_test_expect_output "create-game (full)" "$MGC_BIN create-game --help" "FRAMEWORK"
run_test_expect_output "cre-g (alias)" "$MGC_BIN cre-g --help" "FRAMEWORK"

run_test_expect_output "create-clo (full)" "$MGC_BIN create-clo --help" "FRAMEWORK"
run_test_expect_output "cre-c (alias)" "$MGC_BIN cre-c --help" "FRAMEWORK"

run_test_expect_output "create-cicd (full)" "$MGC_BIN create-cicd --help" "FRAMEWORK"
run_test_expect_output "cre-ci (alias)" "$MGC_BIN cre-ci --help" "FRAMEWORK"

run_test_expect_output "create-iot (full)" "$MGC_BIN create-iot --help" "FRAMEWORK"
run_test_expect_output "cre-i (alias)" "$MGC_BIN cre-i --help" "FRAMEWORK"

run_test_expect_output "create-hardware (full)" "$MGC_BIN create-hardware --help" "FRAMEWORK"
run_test_expect_output "cre-h (alias)" "$MGC_BIN cre-h --help" "FRAMEWORK"

# === INSTALL COMMANDS (full + alias) === — lệnh cài đặt (đầy đủ + alias)
run_test_expect_output "install (full)" "$MGC_BIN install --help" "Install"
run_test_expect_output "i (alias)" "$MGC_BIN i --help" "Install"

run_test_expect_output "install-web (full)" "$MGC_BIN install-web --help" "Install web"
run_test_expect_output "i-web (alias)" "$MGC_BIN i-web --help" "Install web"

run_test_expect_output "install-game (full)" "$MGC_BIN install-game --help" "Install game"
run_test_expect_output "i-game (alias)" "$MGC_BIN i-game --help" "Install game"

run_test_expect_output "install-ai (full)" "$MGC_BIN install-ai --help" "Install AI"
run_test_expect_output "i-ai (alias)" "$MGC_BIN i-ai --help" "Install AI"

run_test_expect_output "install-clo (full)" "$MGC_BIN install-clo --help" "Install cloud"
run_test_expect_output "i-clo (alias)" "$MGC_BIN i-clo --help" "Install cloud"

run_test_expect_output "install-cicd (full)" "$MGC_BIN install-cicd --help" "Install CI/CD"
run_test_expect_output "i-cicd (alias)" "$MGC_BIN i-cicd --help" "Install CI/CD"

run_test_expect_output "install-iot (full)" "$MGC_BIN install-iot --help" "Install IoT"
run_test_expect_output "i-iot (alias)" "$MGC_BIN i-iot --help" "Install IoT"

run_test_expect_output "install-app (full)" "$MGC_BIN install-app --help" "Install app"
run_test_expect_output "i-app (alias)" "$MGC_BIN i-app --help" "Install app"

run_test_expect_output "install-lib (full)" "$MGC_BIN install-lib --help" "Install lib"
run_test_expect_output "i-lib (alias)" "$MGC_BIN i-lib --help" "Install lib"

run_test_expect_output "install-hardware (full)" "$MGC_BIN install-hardware --help" "Install hardware"
run_test_expect_output "i-hardware (alias)" "$MGC_BIN i-hardware --help" "Install hardware"

# === ADD COMMANDS === — lệnh thêm package
run_test_expect_output "add-web (full)" "$MGC_BIN add-web --help" "Add web"
run_test_expect_output "add-game (full)" "$MGC_BIN add-game --help" "Add game"
run_test_expect_output "add-ai (full)" "$MGC_BIN add-ai --help" "Add AI"
run_test_expect_output "add-lib (full)" "$MGC_BIN add-lib --help" "Add lib"

# === BUILD/DEV COMMANDS === — lệnh build/dev
run_test_expect_output "dev (full)" "$MGC_BIN dev --help" "dev"
run_test_expect_output "build (full)" "$MGC_BIN build --help" "build"

# === AUDIT/SECURITY === — kiểm tra bảo mật
run_test_expect_output "audit (full)" "$MGC_BIN audit --help" "Audit"

# === CACHE === — quản lý cache
run_test_expect_output "cache status" "$MGC_BIN cache status --help" "status"
run_test_expect_output "cache clean" "$MGC_BIN cache clean --help" "clean"

# === STORE === — quản lý store
run_test_expect_output "store status" "$MGC_BIN store status --help" "status"
run_test_expect_output "store prune" "$MGC_BIN store prune --help" "prune"

# === DOCTOR === — chẩn đoán
run_test_expect_output "doctor (full)" "$MGC_BIN doctor --help" "doctor"

# === SBOM === — Software Bill of Materials
run_test_expect_output "sbom (full)" "$MGC_BIN sbom --help" "SBOM"

# === TEMPLATE === — quản lý template
run_test_expect_output "template list" "$MGC_BIN template list --help" "list"
run_test_expect_output "template fetch" "$MGC_BIN template fetch --help" "fetch"

# === CONFIG === — cấu hình
run_test_expect_output "config (full)" "$MGC_BIN config --help" "configuration"
run_test_expect_output "c (alias)" "$MGC_BIN c --help" "configuration"

# === INIT === — khởi tạo project
run_test_expect_output "init (full)" "$MGC_BIN init --help" "Interactive project wizard"

# === PUBLISH === — xuất bản
run_test_expect_output "publish (full)" "$MGC_BIN publish --help" "Publish"

# === OUTDATED === — kiểm tra package cũ
run_test_expect_output "outdated (full)" "$MGC_BIN outdated --help" "outdated"

# === SEARCH === — tìm kiếm package
run_test_expect_output "search (full)" "$MGC_BIN search --help" "Search"

# === INFO === — thông tin package
run_test_expect_output "info (full)" "$MGC_BIN info --help" "package information"

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
