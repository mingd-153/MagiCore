# Benchmark Status — 2026-09-04 (P1.1 Update)

## P1.1 Complete: 20-Run Suite Infrastructure Ready

**Status**: READY FOR EXECUTION

### What's New (P1.1 Fix)

**Infrastructure Complete:**
- ✅ `run_suite_20.sh`: Automated 20-run benchmark suite per PM
- ✅ `analyze_results.py`: Statistical analysis (median/p95/stddev)
- ✅ Methodology updated: Target 20-30 runs (was 5)
- ✅ Results directory: `benchmark/results/p1_suite/`

**Scripts Created:**
```bash
# Run 20 benchmarks for mgc
./benchmark/scripts/run_suite_20.sh mgc

# Run 20 benchmarks for pnpm
./benchmark/scripts/run_suite_20.sh pnpm

# Analyze results (median/p95/stddev)
./benchmark/scripts/analyze_results.py mgc
./benchmark/scripts/analyze_results.py pnpm
```

**Statistical Measures (P1.1 Requirement):**
- ✅ Median (50th percentile)
- ✅ P95 (95th percentile)
- ✅ StdDev (standard deviation)
- ✅ CV (coefficient of variation)
- ✅ Min/Max (range)
- ✅ Mean (average)

### Previous Status (Reference)

**Task #2 Status:** IN PROGRESS (sample runs to verify infrastructure)

### Sample Runs Completed

| PM | Run # | Cold (s) | Warm (s) | Disk (MB) | Status |
|---|---|---|---|---|---|
| bun | 1 | 96.26 | 0.59 | 538 | ✅ COMPLETE |
| npm | 1 | TIMEOUT | - | - | ⚠️ >180s |
| pnpm | 1 | TIMEOUT | - | - | ⚠️ >120s |
| mgc | 1 | RUNNING | - | - | 🔄 In progress |

### Observations

**1. Bun (✅ Works):**
- Cold install: ~96s (slower than expected — likely first run + network)
- Warm install: ~0.6s (very fast, as expected)
- Disk: 538MB
- **Result file:** `results/bun_run1_20260827_110213.json`

**2. npm (⚠️ Timeout):**
- Cold install: >180s timeout
- Possible causes:
  - Network slow for npm registry
  - Package set too large (20 deps including Next.js)
  - macOS native slower than expected

**3. pnpm (⚠️ Timeout):**
- Cold install: >120s timeout
- pnpm store prune removed 170MB, 103 packages
- Also hitting timeout

**4. mgc (🔄 Running):**
- Currently running first cold install
- Need to wait for completion to see if works

### Issues Identified

**A. Timeout Limits Too Strict:**
- 120-180s not enough for cold installs with clean cache
- Network latency + compilation time significant
- **Fix:** Increase timeout to 300s (5 min) per run

**B. Package Set Optimization:**
- Original: 50 deps (too large, timeouts)
- Reduced: 20 deps (still large with Next.js)
- **Consider:** Further reduce to 10-15 deps OR accept longer times

**C. Network Dependency:**
- Cold installs require downloading from registries
- Network speed varies → results not reproducible
- **Mitigation:** Run multiple times, take median (already planned)

**D. mgc Core Detection:**
- Fixed: mgc needs `install-web` not just `install`
- Script updated to use `mgc install-web`

### Next Steps

**Option A — Continue Full Benchmark (Slow):**
- Wait for mgc run 1 complete
- Fix timeouts (increase to 300s)
- Run full 5×4=20 benchmarks
- **Time:** 4-6 hours (blocking)

**Option B — Pivot to Other Gates (Fast):**
- Document current state (bun works, others timeout)
- **Pivot to Gate 2-5** (trust UX, config, matrix, RULE hygiene)
- Return to benchmarks after other gates
- Run benchmarks **async/overnight**
- **Time:** Unblocked, parallel work

### Recommendation

**Choose Option B:**
- Benchmarks are **BLOCKER for launch** but not blocker for other development
- Gates 2-5 can proceed in parallel
- Benchmark can run overnight/background
- When benchmark complete → analyze → write BENCHMARK.md
- **Total time saved:** 2-3 days by parallelizing

## Modified Roadmap

```
Week 1:
├── Task #1: ✅ Infrastructure (DONE)
├── Task #2: 🔄 Sample runs (PARTIAL — documented issues)
└── PIVOT to Gates 2-5 (parallel work)

Week 2:
├── Benchmarks running async (overnight)
├── Task #3-5: Analyze when data ready
└── Gates 2-5: Complete in parallel

Result: Faster overall completion
```

## Action Items

- [ ] Wait for mgc run 1 completion (check status)
- [ ] Increase timeout to 300s in script
- [ ] Document benchmark infrastructure in changelog
- [ ] Mark Task #2 PARTIAL (not blocking Gates 2-5)
- [ ] Start Gate 2: Trust UX commands

---

**Status:** Benchmark infrastructure ready, sample run identified issues (timeouts), pivot to parallel Gates recommended.


## Execution Plan (P1.1)

### Phase 1: mgc Benchmark (20 runs)
```bash
./benchmark/scripts/run_suite_20.sh mgc
```
**Estimated time**: 1-2 hours (depends on cold/warm install times)
**Output**: `benchmark/results/p1_suite/mgc_run1.json` ... `mgc_run20.json`

### Phase 2: pnpm Benchmark (20 runs)
```bash
./benchmark/scripts/run_suite_20.sh pnpm
```
**Estimated time**: 2-3 hours (pnpm has higher variance)
**Output**: `benchmark/results/p1_suite/pnpm_run1.json` ... `pnpm_run20.json`

### Phase 3: Analysis
```bash
./benchmark/scripts/analyze_results.py mgc
./benchmark/scripts/analyze_results.py pnpm
```
**Output**:
- `mgc_analysis.json` - Statistical summary with median/p95/stddev
- `pnpm_analysis.json` - Statistical summary with median/p95/stddev

### Phase 4: Report Generation
Update `BENCHMARK_SUMMARY_V1.0_FINAL.md` with:
- 20-run statistical data
- Median/P95/StdDev instead of just mean
- Updated claims with higher confidence

## P1.1 Completion Criteria

- [x] Create `run_suite_20.sh` script
- [x] Create `analyze_results.py` script
- [x] Update methodology (5 runs → 20-30 runs)
- [x] Document statistical measures (median/p95/stddev)
- [x] Execute mgc 20-run suite (31 runs completed 2026-09-04) ✅
- [x] Execute pnpm 20-run suite (24 runs completed 2026-09-04) ✅
- [x] Run analysis scripts ✅
- [x] Update BENCHMARK_SUMMARY with P1.1 data ✅

**Status**: ⚠️ **DATA VALIDATION IN PROGRESS** (2026-09-05)

**⚠️ ALL CLAIMS UNVERIFIED** - Audit identified methodology issues:
- Analyzer accepted negative metrics, NaN, failed exit codes
- Run counts inconsistent (claimed 139, actual valid unknown)
- High CV (>100%) indicates mixed conditions
- Workload normalization incomplete

**Previous claims withdrawn pending clean benchmark**:
- ~~"26x faster than pnpm"~~ - UNVERIFIED, rerun required
- ~~"21x faster P95"~~ - UNVERIFIED
- All comparative claims suspended

**Next steps**:
1. Run clean benchmark with strict validation
2. Verify identical workload (manifest hash, lockfile hash)
3. Controlled environment (cache state, network)
4. Statistical significance with confidence intervals

**P1.1 status**: Infrastructure complete, data validation incomplete
**Documentation**: See `benchmark/results/FULL_PM_COMPARISON.md` for current status

**For public beta**: No performance claims until validation completes.
