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

---

## Observed Sample (2026-09-06, slow-network day — NOT a claim)

**Conditions**: macOS arm64, isolated per-run HOME/store, 3-run observed sample,
registry.npmjs.org metadata latency 5-12s/request from this network (verified
with curl — affects BOTH tools equally). Workload: react + react-dom + lodash.

| Tool | Runs (ms) | Observed median |
|---|---|---|
| pnpm 10.x | 10614, 9843, 11261 | 10614 ms |
| mgc 1.1.0-rc.3 | 11211, 16404, 10308 | 11211 ms |

**Interpretation (honest)**: on a metadata-latency-dominated network the gap
shrinks to ~5% (earlier fast-network 10-run sample showed ~20% cold gap).
mgc cold path is NOT yet faster than pnpm; the remaining gap is dominated by
registry round-trips, not local work (resolver solve: ~90% of mgc time is
metadata fetch; local materialize: 15ms for 1137 files via reflinks).

**Still valid guidance**: no public performance claim until the strict 20+ run
benchmark completes on a stable network.
