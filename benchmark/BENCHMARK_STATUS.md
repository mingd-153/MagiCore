# Benchmark Status — 2026-08-27

## Current Progress

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
