# Core-Web Platform Lab

Last updated: 2026-07-14
Branch: `core-web-platform-lab`
Base branch: `development`

## Goal

Create a dedicated workflow lane for `core-web` that is separate from the normal template/scaffold lane, with four concrete purposes:

1. Build a codebase-reading layer specialized for AI agents so repeated scans cost less context.
2. Build a dedicated test/benchmark lane for frontend, backend, monorepo, and fullstack outputs.
3. Compare MegaGate core-web against external PM/runtime/tooling baselines with repeatable numbers.
4. Run security review from multiple angles: secret leaks, vuln scans, SBOM, static analysis, and adversarial checks.

---

## Current Local Audit Snapshot

### Core areas already identified

- `adapters/web/`
  - web adapter, cache logic, integrity checks, benchmark matrix
- `cli/src/commands/core/web.rs`
  - create/add/install/dev routing for core-web
- `templates/web/`
  - FE / BE / monorepo / fullstack template surface
- `scripts/`
  - generation, benchmark, install, release, and template support scripts

### Current strengths

- adapter correctness is significantly better than earlier passes
- extracted package root now has marker-based integrity validation
- benchmark matrix now covers:
  - cold local cache
  - warm reinstall
  - cold online registry
  - shared cache bootstrap
  - offline cached install
- heavy benchmark profile now exists for larger dependency graphs and version conflicts

### Current risks still visible

- cold path on heavy graphs is still expensive and noisy
- benchmark governance is improving but not yet final
- security posture is better than prototype stage, but not yet product-final
- lifecycle/cleanup policy for shared cache is now present as best-effort pruning, but still needs harder operational validation
- adapter loopback/network fixtures can fail inside restricted sandbox mode and should be validated with the dedicated lab lane outside sandbox before treating them as product regressions

---

## Recommended External Repos / Tools

These are the candidate repos/tools for the dedicated `core-web` lab. They are grouped by purpose.

### A. Codebase compression / AI-agent reading

#### Primary candidate: RepoGraph
- Repo: `ozyyshr/RepoGraph`
- Why:
  - repository-level graph for code understanding
  - designed for AI software engineering context
  - can reduce repeated full-tree reading by turning repo structure into graph artifacts
- Intended role in MegaGate:
  - dedicated install for `core-web` code graph generation
  - generate compact artifacts for agent retrieval instead of raw repeated scans

#### Secondary candidate: OpenGrok
- Repo: `oracle/opengrok`
- Why:
  - mature code search + cross-reference engine
  - useful for human review and code navigation
- Intended role in MegaGate:
  - optional self-hosted searchable mirror for the core-web lane

#### Optional fast-search alternative: Zoekt
- Repo: `google/zoekt`
- Why:
  - very fast code search engine
  - good candidate if low-latency search matters more than cross-reference UI

### B. Benchmark / speed / comparison

#### Primary candidate: hyperfine
- Repo: `sharkdp/hyperfine`
- Why:
  - repeatable CLI benchmarking
  - warmup, prepare hooks, export formats
- Intended role in MegaGate:
  - compare `mg install`, `mg add`, `mg create`, `mg dev` against `pnpm`, `bun`, and others

### C. SBOM / dependency visibility

#### Primary candidate: Syft
- Repo: `anchore/syft`
- Why:
  - SBOM generation for filesystems and projects
- Intended role in MegaGate:
  - snapshot dependency surfaces for FE/BE/mono/full outputs
  - feed later security comparison

### D. Vulnerability / secret / static security

#### Trivy
- Repo: `aquasecurity/trivy`
- Intended role:
  - vuln scan, secrets, config review, SBOM-aware checks

#### Gitleaks
- Repo: `gitleaks/gitleaks`
- Intended role:
  - secrets detection in repo and generated outputs

#### pip-audit
- Repo: `pypa/pip-audit`
- Intended role:
  - Python output audit for Django / Flask / FastAPI templates and fullstack cases

#### SonarQube
- Repo: `SonarSource/sonarqube`
- Intended role:
  - static analysis, maintainability, reliability, security review across mixed-language code

---

## Recommendation

If only one external repo/tool is chosen first for the dedicated core-web reading lane:

**Choose `RepoGraph` first.**

Reason:
- it is the closest fit to the exact requirement of helping AI agents read/search the codebase with less repeated token cost
- it matches repository-level software engineering better than generic grep/search alone

If one additional human-facing code navigation tool is added:

**Choose `OpenGrok` second.**

Reason:
- better for structured manual inspection and cross-reference browsing

---

## New Workflow Proposal

### Phase 1 — Install / Read Layer

Build a dedicated `core-web` reading lane:

- install RepoGraph against the `core-web` surface
- generate graph artifacts for:
  - `adapters/web`
  - `cli/src/commands/core/web.rs`
  - `templates/web`
  - benchmark and generation scripts relevant to web
- define a stable output directory for generated reading artifacts
- avoid forcing agents to re-scan the whole tree on every pass

Suggested internal directory:

```text
tools/core-web-lab/
  graph/
  indexes/
  manifests/
  reports/
  benchmarks/
  security/
```

### Phase 2 — Test Lane

Build a new, isolated test lane for `core-web`, separate from the normal project tests.

Test families:

- scaffold correctness
- install/add/remove/update flows
- benchmark:
  - create
  - install
  - add
  - dev startup
- cache behavior
- shared cache behavior
- lockfile behavior
- node_modules smart materialization
- memory usage
- cold/warm/offline/shared-cache scenarios

Output matrix:

- FE
- BE
- monorepo
- fullstack

### Phase 3 — Competitive Comparison

Compare MegaGate against:

- pnpm
- bun
- npm
- vite-level frontend startup expectations where relevant

Metrics:

- install speed
- warm reinstall speed
- offline resilience
- cache reuse
- disk footprint
- node_modules layout efficiency
- generated project startup speed
- template completeness

### Phase 4 — Security Attack / Audit Lane

Run multiple review passes:

- secret scan
- vuln scan
- SBOM generation
- static analysis
- adversarial install/corrupt-cache tests
- stale metadata and retry abuse cases
- dependency confusion style review on package-resolution assumptions

Expected output:

- issue
- impact
- reproduction
- fix status
- regression test status

---

## Latest Verification Snapshot

### Adapter and CLI tests

- `bash tools/core-web-lab/run-tests.sh`
  - adapter lane: **32/32 passed**
  - CLI core-web lane: **25/25 passed**
- restricted sandbox still produces false negatives on some loopback fixtures; the dedicated lab lane outside sandbox is the source of truth

Interpretation:

- current adapter failures seen in restricted mode are environmental, not confirmed product regressions
- the dedicated lab lane is now the correct source of truth for network/loopback-sensitive adapter verification

### Quick benchmark matrix

Latest quick matrix:

| Scenario | Median | Avg |
|---|---:|---:|
| cold-local-cache | 189.97ms | 195.42ms |
| warm-reinstall | 3.60ms | 3.66ms |
| cold-online-registry | 183.12ms | 181.80ms |
| shared-cache-bootstrap | 179.25ms | 181.46ms |
| offline-cached-install | 31.55ms | 31.57ms |

Takeaway:

- warm reinstall remains excellent
- offline cached install is healthy
- cold path is still the main performance debt if the target is to seriously pressure Bun/pnpm class tooling

### Normalized workspace benchmark lane

New dedicated lane:

- `bash tools/core-web-lab/run-workspace-bench.sh monorepo-basic`
- `bash tools/core-web-lab/run-workspace-bench.sh monorepo-heavy`

This lane measures three states with a dedicated `MEGAGATE_SHARED_CACHE_DIR`:

1. cold project + empty shared cache
2. warm reinstall in the same project
3. fresh project with warmed shared cache

Latest `monorepo-basic` result:

| Scenario | real(s) |
|---|---:|
| cold-project-empty-shared-cache | 40.66 |
| warm-reinstall-same-project | 0.07 |
| fresh-project-warm-shared-cache | 1.71 |

Latest `monorepo-heavy` result:

| Scenario | real(s) |
|---|---:|
| cold-project-empty-shared-cache | 51.58 |
| warm-reinstall-same-project | 0.05 |
| fresh-project-warm-shared-cache | 3.19 |

Takeaway:

- warm reinstall is now effectively near-zero wall time in the current lab fixture
- fresh-project performance benefits heavily from a warm shared cache
- `monorepo-heavy` fresh-project warm shared-cache is about `16.17x` faster than the measured cold-project empty shared-cache path
- cold path remains the major debt by a very large margin
- cold-path parity for `monorepo-heavy` is now restored: all three scenarios produce the same workspace footprint (`7490` files / `132629704` bytes)
- the earlier mismatch was traced back to shared extracted-root races plus benchmark runs using a stale `target/debug/mg` binary; both were corrected in this pass
- additional cold-path tuning now includes per-package tarball prefetch locks and a configurable `MEGAGATE_WEB_DOWNLOAD_CONCURRENCY` limit, but current measurements still place `monorepo-heavy` cold installs roughly in the `50s–60s` band, so the largest remaining problem is still raw cold-start speed rather than correctness

### Monorepo-heavy smoke

Latest `mg` smoke for `monorepo-heavy`:

- exit: `0`
- wall time: `16.60s`
- workspace `node_modules` total files: `7490`
- workspace `node_modules` total bytes: `132629704`
- root `mg.lock`: **present**
- per-workspace `mg.lock`: **present in every child project**

Important behavior confirmed:

- MegaGate now installs all monorepo child projects
- local `workspace:*` dependencies are linked back to local packages via symlink
- MegaGate now writes a unified root `mg.lock` and `mg.lock.sha256` for monorepo projects
- child workspace locks are still preserved, so the current model is dual-layer rather than root-only
- monorepo web installs now run with bounded parallelism, which reduced `monorepo-heavy` wall time from `22.04s` to `16.60s` in the current lab fixture

Latest `mg` smoke for `monorepo-basic`:

- exit: `0`
- wall time: `1.56s`
- workspace `node_modules` total files: `3679`
- workspace `node_modules` total bytes: `62023282`
- root `mg.lock`: **present**

### Cross-PM heavy workspace comparison

`monorepo-heavy` current smoke picture:

| PM | Exit | real(s) | Effective layout note |
|---|---:|---:|---|
| mg | 0 | 16.60 | installs all child workspaces, links local packages, emits unified root lockfile, and now uses bounded parallel monorepo install |
| npm | 1 | 0.35 | fails on `workspace:*` |
| pnpm | 0 | 0.49 | warns that `package.json workspaces` is unsupported without `pnpm-workspace.yaml`; does not materialize child workspace layout under current fixture semantics |
| bun | 0 | 0.93 | creates root-oriented layout quickly, but current smoke report does not yet prove equivalent child materialization semantics |

Interpretation:

- MegaGate is functionally ahead of plain `npm` for this exact workspace shape
- MegaGate monorepo wall-clock improved materially after switching workspace package installs from strict sequential execution to bounded parallel execution
- comparison with `pnpm` and `bun` is still not apples-to-apples until fixture normalization and workspace-semantic parity checks are stricter
- speed alone does not yet support a “competitive with Bun/pnpm” claim on heavy cold installs

---

## First Implementation Pass

### What is now built

A dedicated local lane now exists at [`tools/core-web-lab`](/Users/doanmihh/Documents/Workspace/MegaGate/tools/core-web-lab/README.md:1) with:

- bootstrap script
- read-layer script
- benchmark runner wrapper
- security runner wrapper
- orchestration manifest
- stable output folders for graph / indexes / manifests / reports / benchmarks / security

Key files:

- [`tools/core-web-lab/manifest.toml`](/Users/doanmihh/Documents/Workspace/MegaGate/tools/core-web-lab/manifest.toml:1)
- [`tools/core-web-lab/bootstrap.sh`](/Users/doanmihh/Documents/Workspace/MegaGate/tools/core-web-lab/bootstrap.sh:1)
- [`tools/core-web-lab/run-read-layer.sh`](/Users/doanmihh/Documents/Workspace/MegaGate/tools/core-web-lab/run-read-layer.sh:1)
- [`tools/core-web-lab/run-benchmarks.sh`](/Users/doanmihh/Documents/Workspace/MegaGate/tools/core-web-lab/run-benchmarks.sh:1)
- [`tools/core-web-lab/run-security.sh`](/Users/doanmihh/Documents/Workspace/MegaGate/tools/core-web-lab/run-security.sh:1)
- [`tools/core-web-lab/run-all.sh`](/Users/doanmihh/Documents/Workspace/MegaGate/tools/core-web-lab/run-all.sh:1)

### First measured outputs

- bootstrap surface snapshot:
  - branch: `core-web-platform-lab`
  - commit: `f2e39d60`
  - files in scoped surface: `1843`
- read layer:
  - hash index generated
  - size index generated
  - RepoGraph / OpenGrok / Zoekt currently not installed locally
- security lane:
  - local summary generated
  - no external scanners installed yet (`gitleaks`, `trivy`, `syft`, `pip-audit`, `sonar-scanner`)
- benchmark lane:
  - quick matrix now runs successfully outside sandbox

Quick matrix result from first pass:

| Scenario | Median |
|---|---:|
| cold-local-cache | 181.04 ms |
| warm-reinstall | 3.27 ms |
| cold-online-registry | 179.62 ms |
| shared-cache-bootstrap | 177.45 ms |
| offline-cached-install | 31.20 ms |

### Concrete fixes made during this pass

#### 1. Workspace isolation for the lab lane

Problem:
- root Rust workspace includes `tools/*`
- creating `tools/core-web-lab` broke `cargo` because Cargo expected a crate there

Fix:
- added `exclude = ["tools/core-web-lab"]` to root [`Cargo.toml`](/Users/doanmihh/Documents/Workspace/MegaGate/Cargo.toml:1)

#### 2. Local benchmark harness vs HTTPS policy

Problem:
- benchmark scenarios used loopback HTTP fixtures
- adapter policy now requires HTTPS registry URLs

Fix:
- added a narrow escape hatch in [`adapters/web/src/lib.rs`](/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/lib.rs:160)
- loopback HTTP is only allowed when `MEGAGATE_WEB_ALLOW_INSECURE_LOCALHOST` is enabled
- benchmark binary enables that flag explicitly for lab use in [`adapters/web/src/bin/bench_matrix.rs`](/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/bin/bench_matrix.rs:56)

#### 3. Benchmark fixture integrity correctness

Problem:
- seeded local tarballs existed, but benchmark fixture `ResolvedGraph` carried empty SRI
- cache reuse was rejected by integrity verification and scenarios fell back to network

Fix:
- benchmark fixtures now compute and attach real `sha512` SRI values in [`adapters/web/src/bin/bench_matrix.rs`](/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/bin/bench_matrix.rs:349)

#### 4. Local fixture tarball URL rewrite bug

Problem:
- local registry fixture returned loopback tarball URLs
- adapter rewrote non-HTTPS tarball URLs to npm-style fallback paths
- benchmark online scenario hit `404`

Fix:
- loopback tarball URLs now remain intact when the explicit localhost override flag is on
- fixed in [`adapters/web/src/lib.rs`](/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/lib.rs:1796)

### Second implementation pass

#### A. `mg-web-adapter` test suite is now clean

Current verified result:
- `cargo test -p mg-web-adapter`
- `31 passed / 0 failed`
- validated again through `tools/core-web-lab/run-all.sh quick`

What was fixed:

- test loopback HTTP paths are now allowed automatically under `#[cfg(test)]`
- cache-seeded install tests now attach real tarball SRI instead of `integrity: ""`
- flaky tarball retry test now keeps its integrity outside the spawned server task
- cache/materialization tests now exercise the same integrity path production uses

Meaning:
- adapter correctness is now in a much healthier state than the first lab pass
- the benchmark lane and the adapter test lane are aligned again

#### B. Security lane is still scaffolded, not fully operational yet

What exists:
- output folder and summary lane
- external scanner detection

What is missing:
- actual RepoGraph install flow
- actual gitleaks/trivy/syft/pip-audit/sonar execution
- adversarial cache corruption scripts
- PM comparison baselines (`pnpm`, `bun`, `npm`)

#### C. Performance/product blockers still remain

Even after correctness cleanup, these are still real:

- cold path is still too heavy to claim Bun/pnpm-class competitiveness
- benchmark lane is only using MegaGate-native matrix right now, not external PM baseline comparison yet
- security lane detects tool presence but does not yet run full scanners automatically
- heavy graph / monorepo / reinstall / offline stress still needs a larger matrix than the current quick pass

### Latest orchestration snapshot

From the latest `run-all.sh quick` pass:

- adapter tests: `31/31` passed
- quick matrix:
  - `cold-local-cache`: `178.58 ms`
  - `warm-reinstall`: `3.31 ms`
  - `cold-online-registry`: `176.66 ms`
  - `shared-cache-bootstrap`: `174.64 ms`
  - `offline-cached-install`: `31.93 ms`
- external tools currently missing locally:
  - `repograph`
  - `opengrok`
  - `zoekt-index`
  - `hyperfine`
  - `syft`
  - `gitleaks`
  - `trivy`
  - `pip-audit`
  - `sonar-scanner`

### First PM smoke snapshot

Using the local fixture `react-vite-basic`, the lab now has a first smoke comparison lane.

Important caveat:
- these are warm/local-state smoke numbers, not normalized cold-start benchmarks yet
- `mg` here refers to the MegaGate lab runner, not the unrelated `/usr/bin/mg` command name on macOS
- `pnpm` materialized successfully but returned non-zero because of ignored build policy for `esbuild`

Latest observed smoke results:

| PM | Exit | real(s) | node_modules files | node_modules bytes | Lockfile |
|---|---:|---:|---:|---:|---|
| mg | 0 | 2.87 | 2446 | 61126567 | `mg.lock` |
| npm | 0 | 1.95 | 2375 | 70104282 | `package-lock.json` |
| pnpm | 1 | 0.97 | 2433 | 60570195 | `pnpm-lock.yaml` |
| bun | 0 | 0.18 | 2380 | 60391864 | `bun.lock` |

Interpretation right now:

- MegaGate warm install is already in a usable range, but not yet best-in-class on this fixture
- MegaGate output size is notably smaller than npm on this run
- bun is currently the fastest warm smoke baseline here
- pnpm needs policy-aware interpretation because its non-zero exit does not mean install layout failed

### First monorepo smoke snapshot

Using the local fixture `monorepo-basic`, the lab now has a first workspace-style comparison.

Latest observed smoke results:

| PM | Exit | real(s) | node_modules files | node_modules bytes | Lockfile |
|---|---:|---:|---:|---:|---|
| mg | 0 | 3.35 | 3679 | 62023282 | none |
| npm | 1 | 0.53 | 0 | 0 | none |
| pnpm | 0 | 0.48 | 0 | 0 | `pnpm-lock.yaml` |
| bun | 0 | 0.27 | 0 | 0 | `bun.lock` |

Interpretation right now:

- MegaGate's earlier monorepo root gap has now been **partially fixed**:
  - `mg install` at monorepo root now traverses workspace children
  - `workspace:*` local packages are no longer sent to npm registry
  - local workspace package links are materialized into dependent workspace `node_modules`
- npm fails on this fixture because raw `workspace:*` is not supported in this shape
- pnpm requires its own workspace conventions and did not materialize child workspace installs in this fixture shape
- bun handled the root install quickly, but in this smoke report did not produce child workspace payload comparable to MegaGate's workspace-local materialization

What remains true:

- monorepo support is no longer “missing”, but it is still early-stage
- this is still smoke coverage, not full correctness coverage for complex monorepo graphs
- benchmark parity with Bun/pnpm on monorepo workloads is not proven yet

---

## Immediate Next Actions

1. add explicit lab install flow for RepoGraph and benchmark/security tools
2. create comparison harness for `mg` vs `pnpm` / `bun` / `npm`
3. add adversarial cache / stale metadata / corrupted tarball scenarios into the lab lane
4. expand benchmark profiles for heavy monorepo / reinstall / offline / shared-cache runs

---

## Execution Order

- [x] re-audit current local core-web surface
- [x] identify candidate repos/tools
- [x] create dedicated branch from `development`
- [ ] define `tools/core-web-lab/` structure
- [ ] install/prepare RepoGraph lane for core-web
- [ ] add dedicated benchmark runner set for FE/BE/mono/full
- [ ] add competitive comparison scripts
- [ ] add security scan pipeline
- [ ] produce final product-readiness scorecard

---

## Current Decision

Current branch for this lane:

- `core-web-platform-lab`

Primary external repo to install first:

- `RepoGraph`

Supporting security/comparison stack:

- `hyperfine`
- `syft`
- `trivy`
- `gitleaks`
- `pip-audit`
- `sonarqube`
- optional `opengrok`
- optional `zoekt`

---

## Notes

- This file is the tracking document for the large `core-web` lab effort.
- It is intentionally scoped to `core-web`, not all MegaGate cores yet.
- Later phases should only expand to other cores after the web lane is stable and measurable.
