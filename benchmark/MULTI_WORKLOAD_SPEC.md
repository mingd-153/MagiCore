# Multi-Workload Specification - INVALIDATED

> **⚠️ NOTICE (2026-09-05)**: This workload spec is **INVALIDATED**. Benchmarks using this spec were withdrawn due to validation gaps. Preserved for transparency. **DO NOT USE** for comparative claims.

## Invalidation Reason

See [`BENCHMARK_STATUS.md`](BENCHMARK_STATUS.md) for audit findings and replacement spec.

---

# Original Spec (ARCHIVED)


**Date**: 2026-09-04
**Status**: Framework designed, partial execution
**Requirements**: Test multiple workloads, platforms, and competitors

---

## Workload Matrix

| Workload | Description | Package Count | Use Case | Status |
|----------|-------------|---------------|----------|--------|
| **small** | Minimal Next.js | 5-10 packages | Quick validation | ⏳ TODO |
| **medium** | Current test (unified) | 20 packages | Dev workload | ✅ RUNNING (20 runs) |
| **large** | Monorepo simulation | 100+ packages | Enterprise | ⏳ TODO |
| **real-world** | Clone popular OSS | Varies | Actual projects | ⏳ TODO |

---

## Platform Matrix

| Platform | Arch | OS Version | Runtime | Status |
|----------|------|------------|---------|--------|
| **macOS** | ARM64 (M2) | 26.5 | Node v25 | ✅ IN PROGRESS |
| **Linux** | x86_64 | Ubuntu 22.04 | Node LTS | ⏳ TODO (requires VM/CI) |
| **Windows** | x86_64 | Windows 11 | Node LTS | ⏳ TODO (requires VM/CI) |

---

## Competitor Matrix

| PM | Version | Priority | Status |
|----|---------|----------|--------|
| **mgc** | 1.1.0-rc.1 | P0 | ✅ RUNNING (20 runs) |
| **pnpm** | latest | P1 | ⏳ TODO (after mgc) |
| **npm** | latest | P2 | ⏳ TODO |
| **bun** | latest | P2 | ⏳ TODO |
| **Deno** | latest | P3 | ⏳ TODO (requires setup) |
| **moon** | latest | P3 | ⏳ TODO (requires setup) |
| **proto** | latest | P3 | ⏳ TODO (requires setup) |

---

## Workload Definitions

### Small Workload (5-10 packages)

**Purpose**: Quick smoke test, CI validation

```json
{
  "name": "benchmark-small",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.0.0",
    "react-dom": "^18.0.0",
    "next": "^14.0.0"
  }
}
```

**Expected characteristics**:
- Cold install: <5s (all PMs)
- Total packages (with deps): ~50-100
- Disk: ~50-100MB

**Status**: ⏳ Package.json created (TODO: run benchmarks)

### Medium Workload (20 packages) - CURRENT

**Purpose**: Typical dev project

**File**: `benchmark/env/package-unified.json`

**Characteristics**:
- Cold install: 2-120s (varies by PM)
- Total packages: ~235
- Disk: 295-538MB

**Status**: ✅ 20 runs IN PROGRESS for mgc

### Large Workload (100+ packages)

**Purpose**: Monorepo / enterprise simulation

```json
{
  "name": "benchmark-large",
  "version": "1.0.0",
  "dependencies": {
    "@babel/core": "^7.0.0",
    "@babel/preset-env": "^7.0.0",
    "@babel/preset-react": "^7.0.0",
    "@babel/preset-typescript": "^7.0.0",
    "webpack": "^5.0.0",
    "webpack-cli": "^5.0.0",
    "webpack-dev-server": "^4.0.0",
    "react": "^18.0.0",
    "react-dom": "^18.0.0",
    "react-router-dom": "^6.0.0",
    "next": "^14.0.0",
    "typescript": "^5.0.0",
    "eslint": "^8.0.0",
    "prettier": "^3.0.0",
    "jest": "^29.0.0",
    "@testing-library/react": "^14.0.0",
    "@testing-library/jest-dom": "^6.0.0",
    "axios": "^1.0.0",
    "lodash": "^4.17.21",
    "dayjs": "^1.11.0",
    "classnames": "^2.3.2",
    "styled-components": "^6.0.0",
    "@emotion/react": "^11.0.0",
    "@emotion/styled": "^11.0.0",
    "@mui/material": "^5.0.0",
    "@reduxjs/toolkit": "^1.9.0",
    "react-redux": "^8.0.0",
    "zustand": "^4.0.0",
    "swr": "^2.0.0",
    "react-query": "^3.39.0"
  },
  "devDependencies": {
    "@types/react": "^18.0.0",
    "@types/react-dom": "^18.0.0",
    "@types/node": "^20.0.0",
    "@types/lodash": "^4.14.0",
    "@types/jest": "^29.0.0",
    "@typescript-eslint/eslint-plugin": "^6.0.0",
    "@typescript-eslint/parser": "^6.0.0",
    "eslint-config-prettier": "^9.0.0",
    "eslint-plugin-react": "^7.33.0",
    "eslint-plugin-react-hooks": "^4.6.0"
  }
}
```

**Expected characteristics**:
- Cold install: 30s-5min (varies by PM)
- Total packages: ~1000+
- Disk: 500MB-2GB

**Status**: ⏳ Package.json created (TODO: run benchmarks)

### Real-World Workload

**Purpose**: Validate against actual OSS projects

**Candidates**:
1. **Next.js starter**: `create-next-app` default
2. **React admin**: `react-admin` template
3. **Vite + React + TS**: `create-vite` template
4. **Turborepo example**: Vercel turborepo monorepo

**Method**:
```bash
git clone <project>
cd <project>
rm -rf node_modules package-lock.json pnpm-lock.yaml
./benchmark/scripts/run_benchmark.sh mgc 1
```

**Status**: ⏳ TODO (requires clone + test)

---

## Platform Testing Strategy

### macOS (Current Platform) ✅

**Environment**:
- CPU: Apple M2 (8 cores)
- RAM: 16GB
- OS: Darwin 26.5.0
- Node: v25.9.0

**Status**: ✅ Running 20-run suite for mgc medium workload

### Linux (Ubuntu 22.04) ⏳

**Required Setup**:
1. Spin up Ubuntu 22.04 VM (VirtualBox/Parallels)
2. Install: Node LTS, npm, pnpm, bun, mgc
3. Clone benchmark scripts
4. Run: `./run_suite_20.sh mgc && ./run_suite_20.sh pnpm`

**Alternative**: GitHub Actions matrix job

**Status**: ⏳ Requires VM (estimated 3-4h including setup)

### Windows (Windows 11) ⏳

**Required Setup**:
1. Spin up Windows 11 VM
2. Install: Node LTS, npm, pnpm, bun (via Scoop), mgc (via Scoop)
3. Clone benchmark scripts (adjust paths for Windows)
4. Run PowerShell adapted scripts

**Alternative**: GitHub Actions matrix job

**Status**: ⏳ Requires VM (estimated 4-5h including setup + script adaptation)

---

## Competitor Setup Guide

### Deno (P3 Priority)

**Setup**:
```bash
# Install Deno
curl -fsSL https://deno.land/x/install/install.sh | sh

# Create deno.json
{
  "imports": {
    "react": "npm:react@^18.0.0",
    "next": "npm:next@^14.0.0"
  }
}

# Benchmark
deno cache --reload <entrypoint>
```

**Status**: ⏳ Requires Deno setup + benchmark adaptation

### moon (P3 Priority)

**Setup**:
```bash
# Install moon
curl -fsSL https://moonrepo.dev/install/moon.sh | bash

# Create moon.yml workspace
workspace:
  node:
    packageManager: npm
```

**Status**: ⏳ Requires moon setup + benchmark adaptation

### proto (P3 Priority)

**Setup**:
```bash
# Install proto
curl -fsSL https://moonrepo.dev/install/proto.sh | bash

# Use proto to manage Node versions
proto install node
```

**Status**: ⏳ Requires proto setup + benchmark adaptation

---

## Simplified Task 8 Execution Plan

**Full task 8 (10-20h) is too large for single session. Partial delivery:**

### Phase 1: Additional Workload (NOW) ✅

- [x] Create small workload package.json
- [x] Create large workload package.json
- [ ] Run 5 iterations of small workload (mgc only) - **15 min**
- [ ] Document results

### Phase 2: Cross-Platform (Deferred)

- [ ] Linux VM: 20-run mgc + pnpm - **3-4h**
- [ ] Windows VM: 20-run mgc + pnpm - **4-5h**

### Phase 3: Additional Competitors (Deferred)

- [ ] pnpm 20-run on macOS (after mgc finishes) - **2-3h**
- [ ] bun 20-run on macOS - **1-2h**
- [ ] Deno setup + benchmark - **2-3h**

**Total for complete Task 8**: ~15-20 hours
**Delivered now**: Framework + partial (1-2h)
**Remaining**: Execution on VMs + competitors

---

## Partial Delivery: Small Workload Benchmark (NOW)

Execute 5-run validation on small workload to demonstrate framework works:

**Status**: Executing after mgc 20-run finishes (~1h remaining)

---

## Honest Assessment

**What Task 8 Requires (Full)**:
- 3 workloads × 3 platforms × 4 PMs = 36 benchmark suites
- Each suite: 20-30 runs
- Total runs: 720-1080 individual benchmarks
- Estimated time: 20-30 hours (assuming parallelization)

**What We Can Deliver (This Session)**:
- ✅ Framework designed (workload specs, platform matrix, competitor guide)
- ✅ 1 workload (medium) × 1 platform (macOS) × 1 PM (mgc) = 20 runs IN PROGRESS
- ⏳ 1 additional workload (small) × 1 platform (macOS) × 1 PM (mgc) = 5 runs PENDING
- ⏳ Total delivered: 25 runs (3% of full requirement)

**Recommendation**: Accept partial delivery with clear roadmap for completion. Infrastructure ready, execution requires sustained resources (VMs + time).
