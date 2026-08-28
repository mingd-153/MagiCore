# MagiCore Benchmark Methodology

**Version**: 1.0-final  
**Date**: 2026-08-28  
**Status**: Production

---

## Overview

This document describes the methodology used to benchmark MagiCore against other package managers (npm, pnpm, bun). The goal is to provide **honest, reproducible, statistically valid** performance comparisons.

---

## Principles

### 1. Fairness
- **Same manifest**: All PMs test identical package.json (20 packages)
- **Same network**: All tests use same npm registry
- **Same machine**: All tests run on same hardware back-to-back
- **Same conditions**: Fresh cache clean before each cold run

### 2. Reproducibility
- **Scripted**: Automated scripts (no manual runs)
- **Versioned**: All scripts + manifests in git
- **Documented**: Platform specs + methodology recorded
- **Open**: Raw JSON data committed for verification

### 3. Statistical Rigor
- **Multiple runs**: Minimum 5 runs per PM per phase
- **Variance reporting**: Mean + StdDev + CV (Coefficient of Variation)
- **Outlier handling**: Report all runs, note anomalies
- **No cherry-picking**: Report all data, including failures

### 4. Honesty
- **Caveats disclosed**: Known issues documented (vitest crash, cache speedup)
- **Limitations noted**: macOS-only, no Windows/Linux yet
- **Conservative claims**: Use worst-case or average, not best-case
- **Failed runs disclosed**: Timeouts/errors included in analysis

---

## Test Environment

### Hardware Specification
```
CPU:    Apple M2 (8 cores: 4 performance + 4 efficiency)
RAM:    16GB unified memory
Disk:   512GB SSD (APFS)
```

### Software Versions
```
OS:       macOS 26.5 (Darwin 26.5.0)
Node.js:  v25.9.0
mgc:      1.0.0 (commit af41cb7)
pnpm:     Latest stable (system installed)
npm:      Latest stable (system installed)
bun:      Latest stable (system installed)
```

### Network
- **Registry**: https://registry.npmjs.org
- **Connection**: High-speed fiber (not bottleneck)
- **Proxy**: None (direct connection)

---

## Package Selection

### Manifest: package-unified.json

**Total**: 20 direct dependencies (10 prod + 10 dev)

**Production Dependencies** (10):
```json
{
  "react": "^18.2.0",
  "react-dom": "^18.2.0",
  "next": "^14.0.0",
  "axios": "^1.6.0",
  "zod": "^3.22.0",
  "date-fns": "^2.30.0",
  "lodash": "^4.17.21",
  "clsx": "^2.0.0",
  "framer-motion": "^10.16.0",
  "react-hook-form": "^7.48.0"
}
```

**Dev Dependencies** (10):
```json
{
  "@types/node": "^20.9.0",
  "@types/react": "^18.2.0",
  "@types/lodash": "^4.14.0",
  "typescript": "^5.2.0",
  "eslint": "^8.53.0",
  "tailwindcss": "^3.3.0",
  "postcss": "^8.4.0",
  "autoprefixer": "^10.4.0",
  "prettier": "^3.1.0",
  "jest": "^29.7.0"
}
```

### Rationale

**Why 20 packages?**
- Fixed beta workload selected for repeatability
- Represents typical React/Next.js project
- Complex enough to show performance differences
- Not so large that runs take hours

**Why these specific packages?**
- **Real-world use case**: Actual Next.js + React + TypeScript stack
- **Popular**: All packages have 1M+ weekly downloads
- **Complex dependencies**: Next.js has ~200 transitive deps (tests resolver)
- **Mixed types**: UI libs, utilities, build tools, type definitions

**Why jest instead of vitest?**
- **P0 Issue**: mgc v1.0 crashes on vitest (illegal hardware instruction)
- **Workaround**: Use jest (similar functionality, no crashes)
- **Fairness**: All PMs tested with jest (no advantage to any PM)

---

## Benchmark Phases

### Phase 1: Cold Install
**Definition**: Full install with empty cache

**Procedure**:
1. Clean PM cache completely:
   - mgc: `rm -rf ~/.magicore/store ~/.magicore/cache`
   - pnpm: `pnpm store prune`
   - npm: `npm cache clean --force`
   - bun: `rm -rf ~/.bun/install/cache`
2. Wait 1 second (disk settle)
3. Run install command
4. Measure total time (start → node_modules complete)
5. Measure disk usage: `du -sm node_modules`

**What it measures**:
- Resolver performance
- Download speed (network + parallel fetching)
- Extraction/linking speed
- Total end-to-end time

**Why it matters**:
- First-time install experience
- CI/CD build times
- Developer onboarding

### Phase 2: Warm Install
**Definition**: Install with populated cache

**Procedure**:
1. Delete node_modules: `rm -rf node_modules`
2. Keep cache intact (no clean)
3. Wait 1 second
4. Run install command
5. Measure time

**What it measures**:
- Cache efficiency
- Resolver overhead (re-runs even with cache?)
- Link/copy speed from cache

**Why it matters**:
- Daily developer workflow (switching branches)
- CI cache effectiveness

### Phase 3: Offline Install (Optional)
**Definition**: Install with cache, no network access

**Procedure**:
1. Delete node_modules
2. Run install with offline flag (if supported)
3. Measure time

**What it measures**:
- Pure cache → node_modules speed
- No resolver overhead (uses lockfile)

**Why it matters**:
- Reliability (offline development)
- CI reproducibility

### Phase 4: Incremental Add (Optional)
**Definition**: Add one package to existing install

**Procedure**:
1. Full install (warm)
2. Add single package: `mgc add ms` (or `pnpm add ms`)
3. Measure time

**What it measures**:
- Incremental update efficiency
- Lockfile update speed
- Minimal re-resolution

**Why it matters**:
- Daily workflow (adding dependencies)

---

## Measurement Methodology

### Timing
- **Tool**: `gdate +%s.%N` (GNU date with nanosecond precision)
- **Calculation**: `end_time - start_time` (in seconds)
- **Precision**: 3 decimal places (millisecond accuracy)

### Disk Usage
- **Tool**: `du -sm node_modules` (megabytes, no symlink following)
- **Note**: Includes all files, even if hardlinked elsewhere

### Statistics
- **Mean**: Average of all runs
- **StdDev**: Standard deviation (variability)
- **CV**: Coefficient of Variation = (StdDev / Mean) × 100%
  - Low CV (<20%) = consistent
  - High CV (>50%) = high variability

---

## Benchmark Execution

### Script: `quick_bench.sh`

**Location**: `benchmark/scripts/quick_bench.sh`

**Usage**:
```bash
./quick_bench.sh <pm_name> <run_number>
# Example: ./quick_bench.sh mgc 1
```

**What it does**:
1. Creates temporary workspace
2. Copies package-unified.json
3. Cleans PM cache
4. Runs cold install (timed)
5. Measures disk usage
6. Deletes node_modules
7. Runs warm install (timed)
8. Saves JSON results
9. Cleans up workspace

**Output**: JSON file in `benchmark/results/phased/`

### Running Full Suite

**For 5 statistical runs**:
```bash
for i in 1 2 3 4 5; do
  ./quick_bench.sh mgc $i
  sleep 2  # Cool-down between runs
done

for i in 1 2 3 4 5; do
  ./quick_bench.sh pnpm $i
  sleep 2
done
```

**Note**: Each PM is benchmarked separately to avoid cross-contamination.

---

## Data Analysis

### Aggregation Script

**Python analysis** (example):
```python
import json, glob
from statistics import mean, stdev

cold_times = []
for f in glob.glob("mgc_run*.json"):
    data = json.load(open(f))
    cold_times.append(data['cold']['seconds'])

print(f"Mean: {mean(cold_times):.2f}s")
print(f"StdDev: {stdev(cold_times):.2f}s")
print(f"CV: {(stdev(cold_times)/mean(cold_times))*100:.1f}%")
```

### Comparison Calculation

**Speedup factor**:
```
speedup = pnpm_cold_mean / mgc_cold_mean
# Beta dataset inputs: 120.04s / 2.63s; do not publish an absolute multiplier
```

**Percentage faster**:
```
pct_faster = ((pnpm_time - mgc_time) / pnpm_time) × 100
# Example: ((120 - 2.63) / 120) × 100 = 97.8% faster
```

---

## Claims Validation Process

### Before Making a Claim

1. **Run 5+ times**: Statistical significance
2. **Check variance**: CV <50% (acceptable consistency)
3. **Compare fairly**: Same manifest, same machine
4. **Document caveats**: Known issues, limitations
5. **Conservative wording**: Use "up to" or "average", not "always"

### Claim Template

**Good claim (beta scope)**:
> "mgc cold install competitive on test workload (2.6s vs 120s pnpm, 5 runs, macOS M2, 20-package Next.js manifest, beta data)"

**Bad claim**:
> "mgc is dramatically faster than npm!" (no data, cherry-picked scenario, misleading)

### Disclosure Requirements

Every performance claim must include:
- **Sample size**: "5 runs"
- **Platform**: "macOS M2"
- **Scenario**: "cold install, 20 packages"
- **Caveats**: "vitest excluded due to crash"

---

## Known Limitations

### Current Scope
- ✅ macOS M2 (ARM64)
- ❌ Linux (x86_64) - not tested yet
- ❌ Windows - not tested yet

### Package Selection
- ✅ 20-package Next.js/React manifest
- ❌ Larger monorepos (100+ packages) - not tested
- ❌ Non-JS ecosystems (Rust, Python) - N/A

### Benchmark Phases
- ✅ Cold install
- ✅ Warm install
- ⚠️ Offline install (partial - mgc no offline flag)
- ⚠️ Incremental add (optional - not in main claims)

### Known Issues
1. **vitest crash**: P0 issue, workaround with jest
2. **pnpm variance**: High CV (60%) in this dataset; cause not established
3. **mgc warm cache**: Only 23% speedup (expected 30-50%)

---

## Reproducibility Checklist

To reproduce these benchmarks:

- [ ] Clone MagiCore repo (commit af41cb7 or later)
- [ ] Build mgc: `cargo build --release`
- [ ] Install pnpm: `npm install -g pnpm`
- [ ] Verify platform: macOS with M2 (or similar)
- [ ] Run: `./benchmark/scripts/quick_bench.sh mgc 1`
- [ ] Run: `./benchmark/scripts/quick_bench.sh pnpm 1`
- [ ] Repeat 5 times each
- [ ] Analyze with Python script (see Data Analysis)

**Expected results** (within 20% variance):
- mgc cold: 2-3 seconds
- pnpm cold: 60-120 seconds
- mgc warm: 2-2.5 seconds
- pnpm warm: 1.5-2 seconds

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0-alpha | 2026-08-27 | Initial benchmark (19 vs 20 packages, invalid) |
| 1.0-final | 2026-08-28 | Fixed methodology (unified 20 packages, validated) |

---

## References

- Raw data: `benchmark/results/phased/*.json`
- Summary: `benchmark/results/BENCHMARK_SUMMARY_V1.0_FINAL.md`
- Scripts: `benchmark/scripts/quick_bench.sh`
- Manifest: `benchmark/env/package-unified.json`

---

## Contact

Questions about methodology? Issues reproducing?
- Open issue: https://github.com/your-org/magicore/issues
- Tag: `benchmark` + `methodology`
