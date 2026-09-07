#!/usr/bin/env bash
# Code quality audit — Rà soát chất lượng mã nguồn.
# The gate reports evidence from tracked production Rust files and fails closed.
# Gate dùng bằng chứng từ Rust production đã track và mặc định fail-closed.

set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$REPO_ROOT"

echo "=== MagiCore Code Quality Audit ==="
echo "Repo: $REPO_ROOT"
echo

ISSUES_FOUND=0
BLOCKING=0

# A production file is tracked Rust on disk (deleted-but-uncommitted entries
# are skipped) outside every accepted test/benchmark path.
# File production là Rust đã track, còn tồn tại trên đĩa (bỏ entry đã xóa
# nhưng chưa commit), nằm ngoài toàn bộ path test/benchmark hợp lệ.
RUST_FILES=()
while IFS= read -r file; do
    [[ -f "$file" ]] || continue
    RUST_FILES+=("$file")
done < <(git ls-files '*.rs')
PRODUCTION_FILES=()
for file in "${RUST_FILES[@]}"; do
    case "/$file/" in
        */test/*|*/tests/*|*/benches/*|*/examples/*) ;;
        *) PRODUCTION_FILES+=("$file") ;;
    esac
done

count_matches() {
    local pattern="$1"
    if ((${#PRODUCTION_FILES[@]} == 0)); then
        echo 0
        return
    fi
    { rg --count-matches "$pattern" "${PRODUCTION_FILES[@]}" 2>/dev/null || true; } \
        | awk -F: '{ total += $NF } END { print total + 0 }'
}

show_matches() {
    local pattern="$1"
    local limit="${2:-20}"
    if ((${#PRODUCTION_FILES[@]} > 0)); then
        rg -n "$pattern" "${PRODUCTION_FILES[@]}" 2>/dev/null | head -n "$limit" || true
    fi
}

echo "=== 1. Silent Fallbacks (unwrap_or_default) ==="
FALLBACK_COUNT=$(count_matches 'unwrap_or_default')
echo "Production matches: $FALLBACK_COUNT"
if ((FALLBACK_COUNT > 0)); then
    show_matches 'unwrap_or_default'
    echo "WARN: Review each fallback and require an explicit justification."
    ((ISSUES_FOUND += 1))
else
    echo "PASS: No unwrap_or_default in production code."
fi
echo

echo "=== 2. TODO/FIXME Comments ==="
TODO_COUNT=$(count_matches 'TODO|FIXME')
echo "Production matches: $TODO_COUNT"
if ((TODO_COUNT > 0)); then
    show_matches 'TODO|FIXME'
    echo "WARN: Track or remove every production TODO/FIXME."
    ((ISSUES_FOUND += 1))
else
    echo "PASS: No production TODO/FIXME comments."
fi
echo

echo "=== 3. Bilingual Comments (RULE section 7) ==="
COMMENT_FILES=0
for file in "${PRODUCTION_FILES[@]}"; do
    if rg -q '^[[:space:]]*(//|/\*)' "$file"; then
        ((COMMENT_FILES += 1))
    fi
done
echo "Production files containing comments: $COMMENT_FILES"
echo "INFO: Language compliance requires review; this gate does not pretend to detect Vietnamese."
echo

echo "=== 4. Test Placement ==="
INLINE_TEST_COUNT=$(count_matches '#\[cfg\(test\)\][[:space:]]*mod[[:space:]]+tests[[:space:]]*\{')
MISPLACED_TEST_FILES=()
for file in "${RUST_FILES[@]}"; do
    case "/$file/" in
        */test/*|*/tests/*) continue ;;
    esac
    case "$(basename "$file")" in
        test_*.rs|*_test.rs|*_tests.rs) MISPLACED_TEST_FILES+=("$file") ;;
    esac
done
echo "Inline test blocks in production: $INLINE_TEST_COUNT"
echo "Test-named files outside test/ or tests/: ${#MISPLACED_TEST_FILES[@]}"
if ((INLINE_TEST_COUNT > 0 || ${#MISPLACED_TEST_FILES[@]} > 0)); then
    show_matches '#\[cfg\(test\)\][[:space:]]*mod[[:space:]]+tests[[:space:]]*\{'
    printf '%s\n' "${MISPLACED_TEST_FILES[@]}"
    echo "FAIL: RULE requires tests in test/ or tests/."
    ((ISSUES_FOUND += 1, BLOCKING = 1))
else
    echo "PASS: Test placement follows RULE."
fi
echo

echo "=== 5. Dead-Code Suppressions ==="
BROAD_DEAD_CODE_FILES=()
for file in "${PRODUCTION_FILES[@]}"; do
    if rg -q '^#!\[allow\(dead_code\)\]' "$file"; then
        BROAD_DEAD_CODE_FILES+=("$file")
    fi
done
DEAD_CODE_ALLOW_COUNT=$(count_matches '#!?\[allow\(dead_code\)\]')
echo "All dead_code suppressions: $DEAD_CODE_ALLOW_COUNT"
echo "Blanket file-level suppressions: ${#BROAD_DEAD_CODE_FILES[@]}"
if ((${#BROAD_DEAD_CODE_FILES[@]} > 0)); then
    printf '%s\n' "${BROAD_DEAD_CODE_FILES[@]}"
    echo "FAIL: Blanket dead-code suppression makes an unused-code gate unreliable."
    ((ISSUES_FOUND += 1, BLOCKING = 1))
elif ((DEAD_CODE_ALLOW_COUNT > 0)); then
    show_matches '#\[allow\(dead_code\)\]'
    echo "WARN: Every item-level suppression needs a compatibility or phase justification."
    ((ISSUES_FOUND += 1))
else
    echo "PASS: No dead-code suppressions in production code."
fi
echo

echo "=== 6. Compiler Unused Warnings ==="
set +e
CHECK_OUTPUT=$(CARGO_NET_OFFLINE=true cargo check --workspace --all-targets --all-features --locked --message-format=short 2>&1)
CHECK_STATUS=$?
set -e
if ((CHECK_STATUS != 0)); then
    echo "$CHECK_OUTPUT" | tail -n 40
    echo "FAIL: cargo check failed; unused-code result is invalid."
    ((ISSUES_FOUND += 1, BLOCKING = 1))
else
    UNUSED=$(printf '%s\n' "$CHECK_OUTPUT" | rg 'warning.*(unused|dead.code)' || true)
    if [[ -n "$UNUSED" ]]; then
        printf '%s\n' "$UNUSED" | head -n 20
        echo "FAIL: Compiler reported unused production code."
        ((ISSUES_FOUND += 1, BLOCKING = 1))
    else
        echo "PASS: Compiler reported no unused-code warnings."
    fi
fi
echo

echo "=== 7. Crash-Prone Calls in Production ==="
UNWRAP_COUNT=$(count_matches '\.unwrap\(\)')
PANIC_COUNT=$(count_matches 'panic!')
echo ".unwrap() matches: $UNWRAP_COUNT"
echo "panic! matches: $PANIC_COUNT"
if ((UNWRAP_COUNT > 0 || PANIC_COUNT > 0)); then
    show_matches '\.unwrap\(\)|panic!' 20
    echo "WARN: Review each crash path; counts are evidence, not arbitrary pass thresholds."
    ((ISSUES_FOUND += 1))
else
    echo "PASS: No direct unwrap/panic calls in production code."
fi
echo

echo "=== Audit Summary ==="
echo "Issues found: $ISSUES_FOUND"
echo "Blocking: $BLOCKING"
if ((BLOCKING != 0)); then
    echo "FAIL: Code-quality audit found release-blocking evidence."
    exit 1
fi

if ((ISSUES_FOUND == 0)); then
    echo "PASS: All automated checks passed."
else
    echo "WARN: Non-blocking findings require documented review."
fi
