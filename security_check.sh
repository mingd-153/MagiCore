#!/usr/bin/env bash
# Quick security validation - run before commit/deploy
# Usage: ./security_check.sh

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

check() {
    echo -n "Checking $1... "
    local cmd="$2"
    if bash -c "$cmd" >/dev/null 2>&1; then
        echo -e "${GREEN}PASS${NC}"
        return 0
    else
        echo -e "${RED}FAIL${NC}"
        return 1
    fi
}

FAILS=0

CAS_FILE="web/mgpm/crates/mgpm-store/src/store/cas.rs"

echo "═══════════════════════════════════════════════"
echo "   CAS I/O Security Pre-commit Validation"
echo "═══════════════════════════════════════════════"
echo ""

# 1. No unwrap in production code (before test module at line 404)
check "No unwrap in cas.rs (prod)" \
    "awk 'NR<404' \"$CAS_FILE\" | grep -q '\.unwrap()' && exit 1 || exit 0"

# 2. No expect in production
check "No expect in cas.rs (prod)" \
    "awk 'NR<404' \"$CAS_FILE\" | grep -q '\.expect(' && exit 1 || exit 0"

# 3. Uses create_new for atomic writes
check "Atomic create_new in try_create_write" \
    "grep -q 'create_new(true)' \"$CAS_FILE\""

# 4. Verify after write (same fd)
check "Verify using same file handle" \
    "grep -q 'seek.*Start' \"$CAS_FILE\" && grep -q 'read.*buf' \"$CAS_FILE\""

# 5. Symlink checks
check "Export destination symlink check" \
    "grep -q 'check_symlink_ancestors' \"$CAS_FILE\""

check "Import source symlink check" \
    "grep -q 'Self::check_symlink_ancestors.*path' \"$CAS_FILE\""

check "CAS root symlink validation" \
    "grep -q 'cas_path.is_symlink' \"$CAS_FILE\""

# 6. Permissions
check "CAS root 0o700 permissions" \
    "grep -q '0o700' \"$CAS_FILE\""

# 7. SystemTime safe
check "SystemTime unwrap_or_default" \
    "grep -q 'unwrap_or_default' \"$CAS_FILE\""

# 8. No hardcoded secrets
check "No hardcoded secrets" \
    "! grep -ri 'password\|secret\|token\|api_key' \"$CAS_FILE\" | grep -v test | grep -v '// '"

# 9. All tests pass
check "All tests pass" \
    "cd web/mgpm && cargo test -p mgpm-store -- cas::tests 2>&1 | grep -q '0 failed'"

# 10. Clippy clean (only check cas.rs specific errors)
check "Clippy clean (cas.rs)" \
    "cd web/mgpm && cargo clippy -p mgpm-store --lib 2>&1 | grep -E 'error.*cas\.rs' && exit 1 || exit 0"

echo ""
echo "═══════════════════════════════════════════════"
if [ $FAILS -eq 0 ]; then
    echo -e "${GREEN}ALL SECURITY CHECKS PASSED${NC}"
else
    echo -e "${RED}$FAILS CHECKS FAILED${NC}"
    exit 1
fi
echo "═══════════════════════════════════════════════"