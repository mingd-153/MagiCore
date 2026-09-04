#!/usr/bin/env bash
# Code Quality Audit - Task 8/10
# Finds: dead code, silent fallbacks, TODOs, English-only comments, test placement
# Reports issues for manual review (does not auto-delete)

set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$REPO_ROOT"

echo "=== MagiCore Code Quality Audit ==="
echo "Repo: $REPO_ROOT"
echo ""

ISSUES_FOUND=0

# === 1. Find unwrap_or_default (silent fallbacks) ===
echo "=== 1. Silent Fallbacks (unwrap_or_default) ==="
if git grep -n "unwrap_or_default" -- "*.rs" ":(exclude)target"; then
    echo "⚠️  Found unwrap_or_default (potential silent failures)"
    ((ISSUES_FOUND++))
else
    echo "✅ No unwrap_or_default found"
fi
echo ""

# === 2. Find TODO/FIXME that might be stale ===
echo "=== 2. TODO/FIXME Comments ==="
TODO_COUNT=$(git grep -c "TODO\|FIXME" -- "*.rs" ":(exclude)target" | wc -l || echo "0")
if [[ "$TODO_COUNT" -gt 0 ]]; then
    echo "Found $TODO_COUNT files with TODO/FIXME:"
    git grep -n "TODO\|FIXME" -- "*.rs" ":(exclude)target" | head -20
    echo ""
    echo "⚠️  Review TODOs - remove completed ones, document blockers for others"
    ((ISSUES_FOUND++))
else
    echo "✅ No TODO/FIXME found"
fi
echo ""

# === 3. Find English-only comments (RULE §7 violation) ===
echo "=== 3. Comment Song Ngữ (RULE §7 - English + Vietnamese) ==="
echo "Checking for Rust files with comments..."
# This is a heuristic - can't perfectly detect language
# Look for files with many comments but no Vietnamese characters
FILES_WITH_COMMENTS=$(git grep -l "^[[:space:]]*//\|^[[:space:]]*\/\*" -- "*.rs" ":(exclude)target" || echo "")
if [[ -n "$FILES_WITH_COMMENTS" ]]; then
    echo "⚠️  RULE §7 requires bilingual comments (English + Vietnamese)"
    echo "   Review comment compliance manually"
    echo "   Files with comments: $(echo "$FILES_WITH_COMMENTS" | wc -l)"
    ((ISSUES_FOUND++))
else
    echo "✅ No comment files found (or all compliant)"
fi
echo ""

# === 4. Find tests in wrong location ===
echo "=== 4. Test Placement (tests/ vs src/) ==="
TEST_IN_SRC=$(find . -path "*/src/*" -name "*test*.rs" -o -path "*/src/*" -name "*_test.rs" | grep -v "target" || echo "")
if [[ -n "$TEST_IN_SRC" ]]; then
    echo "⚠️  Found test files in src/ (should be in tests/):"
    echo "$TEST_IN_SRC"
    ((ISSUES_FOUND++))
else
    echo "✅ No test files misplaced in src/"
fi
echo ""

# === 5. Find dead code candidates (unused pub functions) ===
echo "=== 5. Dead Code Candidates (unused warnings) ==="
echo "Running cargo check to find unused code..."
if UNUSED=$(cargo check --workspace --message-format=short 2>&1 | grep "warning.*unused" | head -20); then
    if [[ -n "$UNUSED" ]]; then
        echo "⚠️  Found unused code warnings:"
        echo "$UNUSED"
        echo ""
        echo "   Review these warnings - may be dead code or false positives"
        ((ISSUES_FOUND++))
    else
        echo "✅ No unused code warnings"
    fi
else
    echo "✅ No unused code warnings"
fi
echo ""

# === 6. Find .unwrap() without context ===
echo "=== 6. Unwrap Without Context (.unwrap in production code) ==="
UNWRAP_COUNT=$(git grep -n "\.unwrap()" -- "*.rs" ":(exclude)target" ":(exclude)tests" ":(exclude)cli/tests" | wc -l || echo "0")
if [[ "$UNWRAP_COUNT" -gt 10 ]]; then
    echo "⚠️  Found $UNWRAP_COUNT .unwrap() calls outside tests"
    echo "   Sample (first 10):"
    git grep -n "\.unwrap()" -- "*.rs" ":(exclude)target" ":(exclude)tests" ":(exclude)cli/tests" | head -10
    echo ""
    echo "   Consider .expect() with context or Result propagation"
    ((ISSUES_FOUND++))
else
    echo "✅ Unwrap usage acceptable ($UNWRAP_COUNT calls)"
fi
echo ""

# === 7. Find panic! in production code ===
echo "=== 7. Panic in Production Code ==="
PANIC_COUNT=$(git grep -n "panic!" -- "*.rs" ":(exclude)target" ":(exclude)tests" ":(exclude)cli/tests" | wc -l || echo "0")
if [[ "$PANIC_COUNT" -gt 5 ]]; then
    echo "⚠️  Found $PANIC_COUNT panic! calls outside tests"
    echo "   Sample (first 10):"
    git grep -n "panic!" -- "*.rs" ":(exclude)target" ":(exclude)tests" ":(exclude)cli/tests" | head -10
    echo ""
    echo "   Review if these are intentional (e.g., unreachable code)"
    ((ISSUES_FOUND++))
else
    echo "✅ Panic usage acceptable ($PANIC_COUNT calls)"
fi
echo ""

# === Summary ===
echo "=== Audit Summary ==="
echo "Issues found: $ISSUES_FOUND"
echo ""

# Determine if issues are blocking
BLOCKING=0

# Check for blocking issues:
# - TODOs > 50 files (technical debt too high)
# - unwrap() > 3000 calls (risk too high)
# - panic! > 10 in production (stability risk)
if [[ "$TODO_COUNT" -gt 50 ]]; then
    echo "❌ BLOCKING: Too many TODOs ($TODO_COUNT files) - technical debt too high"
    BLOCKING=1
fi

if [[ "$UNWRAP_COUNT" -gt 3000 ]]; then
    echo "❌ BLOCKING: Too many .unwrap() calls ($UNWRAP_COUNT) - crash risk too high"
    BLOCKING=1
fi

if [[ "$PANIC_COUNT" -gt 10 ]]; then
    echo "❌ BLOCKING: Too many panic! calls ($PANIC_COUNT) - stability risk"
    BLOCKING=1
fi

if [[ $BLOCKING -eq 1 ]]; then
    echo ""
    echo "❌ AUDIT FAILED - Blocking issues found"
    echo "Fix blocking issues before release"
    exit 1
fi

if [[ $ISSUES_FOUND -eq 0 ]]; then
    echo "✅ All checks passed - no issues"
    exit 0
else
    echo "⚠️  $ISSUES_FOUND non-blocking issue(s) found"
    echo "Review findings, document decisions"
    echo ""
    echo "This audit does NOT auto-delete code."
    exit 0  # Non-blocking issues don't fail gate
fi
