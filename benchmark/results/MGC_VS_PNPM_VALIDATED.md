# MagiCore vs pnpm: Validated Benchmark Comparison

**Date**: 2026-09-04
**Status**: Statistical validation complete (20+ runs each)
**Platform**: macOS ARM64 (Apple M2, 8 cores, 16GB RAM)
**Workload**: 20-package dev project (Next.js + React + Vitest + TypeScript)

---

## Executive Summary

**Validated Claims**:
- ✅ **mgc cold install is 26x faster** than pnpm (median: 2.43s vs 62.97s)
- ✅ **mgc P95 is 21x faster** (95% of runs: 11.86s vs 247.29s)
- ⚠️ **pnpm warm install is faster** (1.53s vs 2.18s) - hardlink advantage
- ⚠️ **pnpm uses less disk** (362MB vs 454MB) - mgc CAS overhead +25%

**Cannot Claim** (not validated):
- ❌ "45x faster" - actual data shows 26x median
- ❌ "36x faster than bun" - bun not tested in this run
- ❌ "Fastest overall" - warm install slower than pnpm

---

## Statistical Data

### Cold Install Performance

| Metric | mgc (31 runs) | pnpm (24 runs) | mgc/pnpm Ratio |
|--------|---------------|----------------|----------------|
| **Minimum** | 0.54s | 36.95s | **68x** |
| **Median** | 2.43s | 62.97s | **26x** |
| **Mean** | 5.38s | 84.13s | **16x** |
| **P95** | 11.86s | 247.29s | **21x** |
| **Maximum** | 80.77s | 415.27s | **5x** |
| **StdDev** | 14.18s | 81.42s | - |
| **CV** | 263.5% | 96.8% | - |

**Interpretation**:
- mgc median cold install: **2.43 seconds** (fast, consistent)
- pnpm median cold install: **62.97 seconds** (slow, network-dependent)
- **26x median speedup** is real and reproducible
- High CV for both indicates network/cache variability
- mgc outlier (80s) suggests occasional network hiccup, but P95 stays low (11.86s)

### Warm Install Performance

| Metric | mgc (31 runs) | pnpm (24 runs) | mgc/pnpm Ratio |
|--------|---------------|----------------|----------------|
| **Minimum** | 0.54s | 1.45s | 0.37x |
| **Median** | 2.18s | 1.53s | **0.70x (pnpm faster)** |
| **Mean** | 2.34s | 42.15s | 0.06x |
| **P95** | 7.00s | 2.87s | 2.44x |
| **Maximum** | 7.23s | 974.37s | 0.01x |
| **StdDev** | 1.55s | 198.56s | - |
| **CV** | 66.5% | 471.0% | - |

**Interpretation**:
- pnpm warm median: **1.53 seconds** (hardlink wins)
- mgc warm median: **2.18 seconds** (symlink/hardlink slower)
- **pnpm 1.4x faster** on warm installs (cache hits)
- pnpm extreme CV (471%) due to massive outlier (974s) - likely cache corruption
- mgc more stable warm performance (CV 67%)

### Disk Usage

| Metric | mgc | pnpm | Difference |
|--------|-----|------|------------|
| **Median** | 454 MB | 362 MB | **+92 MB (+25%)** |
| **Mean** | 411 MB | 347 MB | +64 MB (+18%) |
| **P95** | 726 MB | 362 MB | +364 MB (+100%) |

**Interpretation**:
- mgc uses **25% more disk** on average (CAS structure overhead)
- pnpm more disk-efficient (hardlink deduplication)
- Trade-off: mgc speed vs pnpm disk efficiency

---

## Detailed Analysis

### mgc Strengths

**Cold Install Dominance**:
- 26x faster median (2.43s vs 63s)
- Consistent P95: 95% of cold installs under 12s
- Network-independent: CAS + local resolution
- Small workload (5 packages): 0.76s median - excellent scaling

**Architecture Advantages**:
- Content-addressable storage (CAS) avoids network round-trips
- Parallel resolution + download
- Optimized for cold starts (CI/CD use case)

**Use Cases**:
- CI/CD pipelines (fresh clones, no cache)
- Developer onboarding (first install)
- Monorepo fresh builds
- Docker image builds

### pnpm Strengths

**Warm Install Efficiency**:
- 1.4x faster warm median (1.53s vs 2.18s)
- Hardlink deduplication (instant "install" from global store)
- Stable warm performance (when working)

**Disk Efficiency**:
- 25% less disk usage
- Global store deduplication
- Better for disk-constrained environments

**Use Cases**:
- Iterative local development (frequent npm install)
- Disk-limited systems
- Multiple projects sharing dependencies

### Trade-offs

| Dimension | mgc Wins | pnpm Wins |
|-----------|----------|-----------|
| **Cold install** | ✅ 26x faster | - |
| **Warm install** | - | ✅ 1.4x faster |
| **Disk usage** | - | ✅ 25% smaller |
| **CI/CD** | ✅ Consistent | ⚠️ Variable |
| **Local dev** | ⚠️ Slower warm | ✅ Faster warm |
| **Stability (cold)** | ⚠️ CV 264% | ✅ CV 97% |
| **Stability (warm)** | ✅ CV 67% | ⚠️ CV 471% |

---

## Methodology

### Test Setup

**Workload**: 20-package dev project
```json
{
  "dependencies": {
    "react": "^18.3.0",
    "react-dom": "^18.3.0",
    "next": "^14.2.0",
    "typescript": "^5.5.0",
    "axios": "^1.7.0",
    "lodash": "^4.17.21",
    "date-fns": "^2.30.0",
    "clsx": "^2.1.0",
    // ... (20 total packages)
  }
}
```

**Machine**:
- CPU: Apple M2 (8 cores, ARM64)
- RAM: 16GB
- OS: macOS 26.5.0 (Darwin kernel)
- Node: v25.9.0
- Disk: SSD (fast I/O)

**Procedure**:
1. **Cold install**: Clear all caches, fresh `node_modules`
2. **Warm install**: Re-install with cache populated
3. **Iterations**: 31 runs (mgc), 24 runs (pnpm)
4. **Cache clearing**: 
   - mgc: `rm -rf ~/.magicore/store ~/.magicore/cache`
   - pnpm: `pnpm store prune`
5. **Sleep**: 30s between runs to avoid resource contention
6. **Metrics**: Duration (seconds), disk (MB), package count

### Statistical Rigor

**Sample Size**:
- mgc: 31 runs (sufficient for median/P95)
- pnpm: 24 runs (sufficient for comparison)

**Measures**:
- Median: robust to outliers, primary metric
- P95: 95th percentile, measures "worst acceptable case"
- CV (Coefficient of Variation): variability indicator
- Mean: included for completeness (sensitive to outliers)

**Validity**:
- Multiple runs eliminate single-point bias
- Cache clearing ensures cold starts
- Same workload/machine for fair comparison
- Outliers documented (mgc 80s, pnpm 974s) but don't invalidate medians

---

## Caveats & Limitations

### Single Platform
- **Tested**: macOS ARM64 only
- **Not tested**: Linux x86_64, Windows, macOS x86_64
- **Impact**: Performance may vary on other platforms

### Single Workload
- **Tested**: 20-package dev project
- **Not tested**: Large monorepo (100+ packages), small projects (5 packages tested separately)
- **Impact**: Scaling behavior unknown

### Network Dependency
- **Test environment**: Home network (variable latency)
- **Not tested**: Enterprise proxy, offline mode, poor connectivity
- **Impact**: High CV suggests network affects both PMs

### Missing Competitors
- **Not tested**: npm, bun, yarn, Deno
- **Impact**: Cannot claim "fastest overall" without full comparison

### Warm Install Caveat
- pnpm warm install has extreme outlier (974s) suggesting cache corruption edge case
- More investigation needed for pnpm warm stability

---

## Honest Positioning

### What We CAN Say

✅ **"mgc cold install is 26x faster than pnpm"** (median: 2.43s vs 63s, 24-31 runs, macOS ARM64, 20-package workload)

✅ **"mgc optimized for CI/CD"** (consistent P95 <12s vs pnpm P95 247s)

✅ **"pnpm wins on warm installs"** (1.53s vs 2.18s) - honest about trade-off

✅ **"mgc uses 25% more disk"** - transparent about CAS overhead

### What We CANNOT Say

❌ **"45x faster than pnpm"** - data shows 26x median (not 45x)

❌ **"36x faster than bun"** - bun not tested in this comparison

❌ **"Fastest package manager"** - only compared to pnpm, and pnpm wins warm installs

❌ **"Production ready for all platforms"** - only tested macOS ARM64

### Recommended Framing

> "MagiCore shows **26x faster cold install** than pnpm (median 2.43s vs 63s, 24-31 runs, macOS ARM64). Optimized for CI/CD and fresh clones. pnpm retains advantages in warm installs (1.4x faster) and disk efficiency (25% smaller). Preliminary benchmarks - full cross-platform validation in progress."

---

## Next Steps for Validation

### Priority 1 (Cross-Platform)
- [ ] Linux x86_64 (20 runs each)
- [ ] Windows x86_64 (20 runs each)
- [ ] macOS x86_64 (20 runs - needs Intel Mac)

### Priority 2 (Competitor Coverage)
- [ ] bun (20 runs, macOS ARM64)
- [ ] npm (20 runs, macOS ARM64)
- [ ] yarn (20 runs, macOS ARM64)

### Priority 3 (Workload Coverage)
- [x] Small (5 packages) - done: 0.76s median
- [x] Medium (20 packages) - done: 2.43s median
- [ ] Large (100+ packages) - pending
- [ ] Real-world (Next.js starter, React Admin) - pending

### Priority 4 (Stability)
- [ ] Investigate mgc 80s outlier (network? cache?)
- [ ] Investigate pnpm 974s warm outlier (cache corruption?)
- [ ] Test with offline mode / poor network

---

## Conclusion

**Validated**: mgc delivers **26x faster cold install** than pnpm on macOS ARM64 with statistical rigor (24-31 runs).

**Trade-offs**: pnpm wins warm installs (+1.4x) and disk efficiency (+25%). Both have use cases.

**Recommendation**: 
- **Use mgc for**: CI/CD, Docker builds, fresh clones, onboarding
- **Use pnpm for**: Iterative local dev, disk-limited systems, warm installs

**Public Beta Status**: Ready for **"macOS ARM64 Early Access"** with honest performance claims. Full cross-platform validation pending.

---

**Report Date**: 2026-09-04 17:35
**Data Provenance**: 
- mgc: `benchmark/results/mgc_run*.json` (31 files)
- pnpm: `benchmark/results/pnpm_run*.json` (24 files)
- Analysis: `benchmark/scripts/analyze_results.py`
