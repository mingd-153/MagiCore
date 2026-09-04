# P1.3: mgc vs Alternatives — PRELIMINARY Data

**Date**: 2026-09-04
**Status**: ⚠️ PRELIMINARY - NOT READY FOR PUBLIC BETA CLAIMS
**Platform**: macOS 26.5, Apple M2, 16GB RAM (single platform only)
**Workload**: 20-package Next.js manifest (single workload only)
**Runs**: 5 per PM (need 20-30 for statistical confidence)

---

## ⚠️ IMPORTANT CAVEATS

**This data is NOT sufficient for public beta claims because:**

1. **Insufficient runs**: Only 5 runs per PM (need 20-30 minimum per P1.1 requirement)
2. **Single platform**: macOS M2 only (no Linux/Windows data)
3. **Single workload**: 20-package Next.js (not representative of monorepo/large projects)
4. **Uncontrolled variables**: Network conditions not controlled (pnpm 60% CV suggests network variance)
5. **Missing competitors**: No Deno, moon, proto comparison
6. **Infrastructure only**: Benchmark scripts ready but 20-30 run data NOT collected

**Status**: Benchmark infrastructure complete. Actual execution pending (estimated 5-7 hours).

---

## Preliminary Results (5 runs - DO NOT CITE)

| Metric | mgc | pnpm | npm | bun |
|--------|-----|------|-----|-----|
| **Cold Install (mean)** | 2.63s | 120.04s | TIMEOUT | 96.26s |
| **Warm Install (mean)** | 2.01s | 1.69s | - | 0.59s |
| **Disk Usage** | 462MB | 360MB | - | 538MB |
| **Consistency (CV cold)** | 25% | 60% | - | - |

### Early Observations (NOT conclusions)

**mgc preliminary data**:
- Cold install: 2.63s average (5 runs on this specific workload)
- Warm install: 2.01s average
- Disk: 462MB (28% larger than pnpm hardlink store)
- Consistency: 25% CV (better than pnpm 60%)

**pnpm preliminary data**:
- Cold install: 120.04s average (high variance 58-232s range)
- Warm install: 1.69s average (faster than mgc by ~0.3s)
- Disk: 360MB (most efficient via hardlinks)
- Consistency: 60% CV (network-bound hypothesis)

**bun preliminary data**:
- Cold install: 96.26s average (1 run only - insufficient)
- Warm install: 0.59s (fastest - zero-copy hardlinks)
- Disk: 538MB (largest footprint)

**npm preliminary data**:
- Timed out (>180s) on cold install

### What This Data CANNOT Claim

❌ "mgc is 45x faster than pnpm" - only true on THIS workload, THIS platform, 5 runs
❌ "mgc dominates cold install" - insufficient cross-platform/cross-workload data
❌ "Production ready for all use cases" - only tested dev workload
❌ "Outperforms alternatives" - missing Deno/moon/proto comparison

### What We Can Say (Honestly)

✅ "On macOS M2 with 20-package Next.js workload (5 runs): mgc averaged 2.63s cold, pnpm 120s"
✅ "Preliminary data suggests mgc may be faster for cold installs on tested configuration"
✅ "Benchmark infrastructure ready for comprehensive 20-30 run validation"
✅ "More data needed across platforms, workloads, and competitors before public claims"

---

## Next Steps for Valid Comparison

**Required before public beta claims:**

1. **Run 20-30 iterations** per PM per workload (current: 5 runs mgc/pnpm, 1 run bun)
2. **Test on 3 platforms**: macOS, Linux (Ubuntu 22.04), Windows 11
3. **Multiple workloads**:
   - Small: 5-10 packages (Next.js minimal)
   - Medium: 20-50 packages (current test)
   - Large: 100+ packages (monorepo simulation)
   - Real-world: Clone popular open-source projects
4. **Add missing competitors**: Deno, moon, proto
5. **Control network**: Local registry or fixed CDN to reduce pnpm variance
6. **Document environment**: CPU, RAM, disk type, network speed for reproducibility
7. **Statistical analysis**: Confidence intervals, outlier handling, distribution analysis

**Estimated effort**: 10-20 hours execution time + 2-4 hours analysis

---

## Methodology Issues (Current)

**Problems with 5-run data:**
- Small sample size → high margin of error
- pnpm 60% CV suggests outliers or network variance
- bun only 1 run → no statistical validity
- npm timeout may be config issue (not fundamental limitation)

**Problems with single platform:**
- macOS M2 has specific file system (APCS) and syscalls
- Linux ext4/btrfs may show different reflink behavior
- Windows NTFS lacks reflink → different code paths

**Problems with single workload:**
- 20 packages not representative of monorepo (1000+ packages)
- Next.js ecosystem may favor specific PM optimizations
- No test of Python, Rust, Go, Java ecosystems

---

## Honest Assessment

**Current data value**: Exploratory only. Shows mgc implementation works and has promising early results.

**Production readiness**: NOT sufficient for public beta claims without comprehensive validation.

**What we actually have**: Benchmark *infrastructure* complete and ready. Benchmark *data* incomplete.

**Recommendation**: Complete 20-30 run suite + multi-platform + multi-workload before making comparative claims in public beta announcement.

---

## File Purpose

This file replaces `MGC_VS_ALTERNATIVES.md` as the honest assessment. Previous file made claims beyond data support. This file documents:
- What data exists (5 runs, 1 platform, 1 workload)
- What data is needed (20-30 runs, 3 platforms, multiple workloads)
- What can/cannot be claimed
- What work remains

**For public beta**: Use generic claims ("fast install", "CAS-based caching") until comprehensive data collected. Do not cite specific performance multiples without statistical backing.
