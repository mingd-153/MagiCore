# mgc vs pnpm Comparison - INVALIDATED

> **⚠️ NOTICE (2026-09-05)**: This document is **INVALIDATED**. All prior benchmark data withdrawn due to validation gaps. Do not cite or use for performance claims.

**Date**: 2026-09-05  
**Status**: ⚠️ **INVALIDATED - DATA UNDER REVIEW**

---

## Audit Findings

User audit (2026-09-05) identified critical validation gaps in original benchmark data:

### Issues Found

1. **Analyzer accepted invalid data**:
   - Negative metrics (duration, disk space)
   - NaN and Infinity values
   - Failed exit codes (non-zero)
   - Missing required fields

2. **Run count discrepancies**:
   - Claimed: 139 total runs
   - Actual valid (strict): Unknown (recount needed)
   - Original mgc data: 0 samples pass new strict validator

3. **High variability**:
   - mgc CV: 229%
   - npm CV: 105%
   - yarn CV: 156%
   - Indicates mixed workload or cache state

4. **Workload validation incomplete**:
   - package_count checked, but not manifest/lockfile hash
   - Cannot prove identical dependency graph
   - PM version, commit SHA not validated

### Analyzer Fixed

Enhanced `benchmark/scripts/analyze_results_strict.py` with:
- ✅ Finite number validation (reject NaN, Inf)
- ✅ Positive duration requirement
- ✅ Exit code validation (only 0 accepted)
- ✅ PM and timestamp validation
- ✅ 15 regression tests (all pass)

**Result**: Original dataset fails new validation. Clean rerun required.

---

## Previous Claims (Withdrawn)

The following claims were made but are now withdrawn pending revalidation:

### Cold Install
- ~~"26x faster than pnpm"~~ - UNVERIFIED
- ~~"21x faster P95"~~ - UNVERIFIED

### Warm Install
- ~~"pnpm 1.4x faster"~~ - UNVERIFIED

### Disk Usage
- ~~"pnpm 25% smaller"~~ - UNVERIFIED

**Reason**: Underlying data does not pass strict validation requirements.

---

## Next Steps

To restore comparative claims, need:

1. **Clean benchmark run**:
   - Minimum 10 runs per PM
   - Identical workload (verified manifest hash)
   - Controlled cache state
   - Single session per PM

2. **Strict validation**:
   - All samples pass analyzer regression tests
   - PM version recorded
   - Commit SHA recorded
   - Session ID unique
   - No failed runs counted as success

3. **Statistical rigor**:
   - CV < 50% for each PM
   - Confidence intervals reported
   - Outlier analysis documented
   - Failed/timeout runs reported separately

4. **Cross-platform validation**:
   - Linux, macOS, Windows
   - x86_64 architecture
   - Multiple workload sizes

---

## Current Recommendation

**For RC-3 release**: Do not make performance claims. Focus on:
- Functional correctness (install works)
- Multi-core support (web/ai/app/lib)
- Package manager compatibility

**For benchmark validation**: Complete clean rerun with strict validation before any public comparative claims.

---

## References

- Original (invalid) data: `benchmark/results/` (various JSON files)
- Strict analyzer: `benchmark/scripts/analyze_results_strict.py`
- Regression tests: `benchmark/scripts/test_analyzer_strict.py`
- Audit report: User feedback 2026-09-05
