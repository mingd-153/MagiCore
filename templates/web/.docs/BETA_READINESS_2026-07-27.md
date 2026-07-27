# Core-Web Beta Readiness Gate

Date: 2026-07-27

## What we just tightened

- blocked `--recursive` from silently doing nothing
- restored a real single-core `mg create` entrypoint for web-only builds
- aligned adapter docs with actual single-core vs multi-core behavior
- rejected `package.json` script wrappers that delegate to `npm`, `pnpm`, `bun`, `yarn`, `npx`, or `bunx`
- wired `--audit-strict` into the real web install path instead of refusing early at dispatch
- tightened local dev bind defaults toward `127.0.0.1` while keeping container runtime on `0.0.0.0`
- replaced remaining React / Solid / Vanilla router placeholders with meaningful starter UI
- re-verified `mg` in both:
  - full multi-core build
  - web-only build

## Verification snapshot

- `cargo test -p mg` ✅
- `cargo test -p mg --no-default-features --features web` ✅
- `cargo test -p mg-web-adapter` ✅
- networked benchmark subset: `benchmark_brutal_results_20260727_205854.md` ✅

This matters because core-web now has to behave correctly in both shipping shapes:

1. MegaGate full binary
2. `megagate-web` style single-core binary

## Beta judgment

### Ready enough for a **beta**

Yes, with constraints.

Core-web is now in a shape where it can be exposed as a beta for:

- scaffold/create
- install/add/remove/update/list
- dev/build/start
- monorepo/fullstack template generation
- web-only and full-binary command surface validation

Architecture rule now enforced:

- MegaGate core-web may execute framework-local binaries such as `vite`, `next`, `nuxt`, `astro`, `ng`, or `tsx`
- MegaGate core-web must not bounce through another package manager to do its work

### Not ready to claim parity/supremacy

Not yet.

It is still not honest to claim:

- faster than Bun on cold online install
- smarter/safer than pnpm across the full dependency lifecycle
- production-finished for every workspace/CI/security edge case

## Latest benchmark snapshot

Sources:

- `benchmark_brutal_results_20260727_203247.md`
- `benchmark_brutal_results_20260727_203858.md`
- `benchmark_brutal_results_20260727_205854.md`
- `benchmark_brutal_results_20260727_210702.md`
- `benchmark_brutal_results_20260727_211124.md`
- `benchmark_brutal_results_20260727_213749.md`
- `benchmark_brutal_results_20260727_215214.md`
- `benchmark_brutal_results_20260727_220040.md`
- `benchmark_brutal_results_20260727_220544.md`
- `benchmark_brutal_results_20260727_221412.md`
- `benchmark_brutal_results_20260727_221748.md`
- `benchmark_brutal_results_20260727_221830.md`
- `benchmark_brutal_results_20260727_221902.md`
- `benchmark_brutal_results_20260727_221941.md`
- `benchmark_brutal_results_20260727_222008.md`
- `benchmark_brutal_results_20260727_222036.md`
- `benchmark_brutal_results_20260727_222727.md`

- empty-cache install:
  - before focused strict-layout fix:
    - mg: `~9.624s`
    - bun: `~8.675s`
    - pnpm: `~5.968s`
  - after removing the strict-layout prefetch wait:
    - mg: `~8.334s`
    - bun: `~8.625s`
    - pnpm: `~5.948s`
  - later network-path recheck after removing forced HTTP/1-only client behavior:
    - mg: `~8.567s`
    - bun: `~6.892s`
    - pnpm: `~6.427s`
  - verdict:
    - MG improved materially on cold path
    - MG does not yet hold a stable cold-path lead over Bun or pnpm
    - cold path is still the clearest blocker to a stronger product claim
- warm install:
  - earlier subset:
    - mg: `~453ms`
    - bun: `~2.632s`
    - pnpm: `~3.856s`
  - latest subset after the strict-layout fix:
    - mg: `~203ms`
    - bun: `~1.969s`
    - pnpm: `~3.839s`
  - later network-path recheck:
    - mg: `~196.9ms`
    - bun: `~1.896s`
    - pnpm: `~3.775s`
  - verdict: MG wins strongly on warm path

Important audit note:

- a later detached-prefetch experiment on **Monday, July 27, 2026 21:11** did not produce a better cold-path outcome and also yielded a failed comparison warmup on the `empty-cache-install` lane
- that experiment was treated as non-promotable and reverted from the beta candidate path

Heavy-graph note:

- on **Monday, July 27, 2026 21:37**, the heavy empty-cache lane still came out far behind:
  - mg: `~30.433s`
  - bun: `~21.735s`
  - pnpm: `~17.471s`
- this confirms the same product-readiness conclusion on a harsher graph shape:
  - steady and warm paths are strong
  - heavy first-run cold install is still not product-claim ready
- on **Monday, July 27, 2026 21:52**, a monorepo cold-orchestration guard landed:
  - multi-target cold monorepo package installs now collapse package-target concurrency to `1`
    unless overridden by `MEGAGATE_WEB_MONOREPO_INSTALL_CONCURRENCY`
  - monorepo lane improved materially in the benchmark subset:
    - mg: `~33.2ms`
    - bun: `~266.0ms`
    - pnpm: `~267.2ms`
  - but the harsher heavy empty-cache lane still remained behind:
    - mg: `~24.256s`
    - bun: `~20.308s`
    - pnpm: `~17.742s`
  - verdict:
    - orchestration for cold monorepo startup is better
    - global heavy cold-path leadership is still not there
- on **Monday, July 27, 2026 22:27**, deeper heavy cold-path instrumentation landed:
  - retained changes:
    - heavy fixture promoted into repo at `tools/core-web-lab/fixtures/heavy-web`
    - `benchmark.sh` now uses that stable fixture instead of regenerating it inline
    - pipeline profiling now separates:
      - `download_ms_total`
      - `extract_ms_total`
      - `queue_wait_ms`
      - `io_ms`
      - top slowest download/extract packages
    - registry client has optional network retry logging
  - measured tuning:
    - download concurrency
      - `24 -> ~20.504s`
      - `32 -> ~28.724s`
      - `48 -> ~22.408s`
    - metadata concurrency
      - `16 -> ~23.112s`
      - `24 -> ~22.960s`
      - `32 -> ~26.088s`
  - verdict:
    - default `24` remains the best measured value for both knobs in this heavy lane
    - the cold heavy bottleneck is not extract-first and not obvious retry-storm behavior
    - it is mainly tarball download scheduling plus queue wait under a large graph
- on **Monday, July 27, 2026 22:42**, the strict cold-path install pipeline was tightened again:
  - retained changes:
    - speculative resolve-time tarball prefetch is now opt-in via `MEGAGATE_WEB_RESOLVE_PREFETCH=1`
    - strict-layout download/extract pipeline now uses bounded `buffer_unordered(...)` backpressure instead of spawning the full graph at once
    - benchmark harness gained a new MG-only lane:
      - `heavy-empty-cache-install-direct`
  - verification:
    - `cargo test -p mg-web-adapter --lib` -> `50/50`
    - `bash -n benchmark.sh`
  - measured result:
    - `benchmark_brutal_results_20260727_224206.md`
    - `heavy-empty-cache-install`
      - mg: `~27.379s`
    - `heavy-empty-cache-install-direct`
      - mg: `~20.968s`
  - verdict:
    - the new direct lane is better for isolating engine cost from benchmark harness/setup overhead
    - this round improves measurement honesty and reduces pipeline task pressure
    - it still does not establish cold-path leadership over Bun or pnpm
- on **Monday, July 27, 2026 22:46**, an extra pipeline task-cap experiment was tested and rejected:
  - trial:
    - cap active pipeline tasks to `download_concurrency_limit()` instead of the looser combined bound
  - measured result:
    - `benchmark_brutal_results_20260727_224557.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~21.772s`
  - compare against the immediately prior kept baseline:
    - `benchmark_brutal_results_20260727_224206.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~20.968s`
  - verdict:
    - non-promotable
    - reverted
- on **Monday, July 27, 2026 22:49**, a small resolver churn reduction landed and stayed:
  - change:
    - after `initial_prefetch_versions`, batch prefetch no longer re-requests package names already present in `prefetched_versions`
  - verification:
    - `cargo test -p mg-resolver` -> `16/16`
    - `cargo test -p mg-web-adapter --lib` -> `50/50`
  - measured result:
    - baseline:
      - `benchmark_brutal_results_20260727_224206.md`
      - `heavy-empty-cache-install-direct`
        - mg: `~20.968s`
    - after fix:
      - `benchmark_brutal_results_20260727_224906.md`
      - `heavy-empty-cache-install-direct`
        - mg: `~20.640s`
  - verdict:
    - small but real improvement
    - promotable
- on **Monday, July 27, 2026 22:52**, an additional metadata dedupe experiment in `prefetch_dependencies(...)` was tested and rejected:
  - trial:
    - fetch metadata once per `source_package_name(...)` and fan out dependency extraction across package versions
  - verification:
    - `cargo test -p mg-web-adapter --lib` -> `50/50`
  - measured result:
    - `benchmark_brutal_results_20260727_225219.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~21.119s`
  - compare against the kept baseline:
    - `benchmark_brutal_results_20260727_224906.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~20.640s`
  - verdict:
    - non-promotable
    - reverted
- on **Monday, July 27, 2026 22:58**, a strict first-materialization cleanup experiment was also rejected:
  - trial:
    - remove strict-layout staging-root churn
    - skip root prune on fresh empty `node_modules`
  - verification:
    - `cargo test -p mg-web-adapter --lib` -> `50/50`
  - measured result:
    - `benchmark_brutal_results_20260727_225807.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~25.543s`
  - compare against the kept baseline:
    - `benchmark_brutal_results_20260727_224906.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~20.640s`
  - verdict:
    - non-promotable
    - reverted
- on **Monday, July 27, 2026 23:02**, a strict dependency-linking leaf-package mkdir deferral experiment was also rejected:
  - trial:
    - skip eager `pkg_local_node_modules` creation for leaf packages with no deps
    - only create the nested `node_modules` dir when entering the dependency-link loop
  - verification:
    - `cargo test -p mg-web-adapter --lib` -> `50/50`
  - measured result:
    - `benchmark_brutal_results_20260727_230223.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~28.418s`
  - compare against the kept baseline:
    - `benchmark_brutal_results_20260727_224906.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~20.640s`
  - verdict:
    - non-promotable
    - reverted
- on **Monday, July 27, 2026 23:05**, a fresh virtual-store fast-path experiment was also rejected:
  - trial:
    - when `.megagate` is empty on first install, skip `installed_package_matches(...)` checks and materialize directly
  - verification:
    - `cargo test -p mg-web-adapter --lib` -> `50/50`
  - measured result:
    - `benchmark_brutal_results_20260727_230526.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~25.514s`
  - compare against the kept baseline:
    - `benchmark_brutal_results_20260727_224906.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~20.640s`
  - verdict:
    - non-promotable
    - reverted
- on **Monday, July 27, 2026 22:14**, speculative strict-layout prefetch was re-tested for large graphs and rejected again:
  - `benchmark_brutal_results_20260727_221412.md`
  - mg: `~29.585s`
  - bun: `~24.456s`
  - pnpm: `~13.394s`
  - verdict:
    - non-promotable
    - reverted
- on **Monday, July 27, 2026 22:27**, a download scheduler priority experiment was also rejected:
  - `benchmark_brutal_results_20260727_222727.md`
  - mg: `~24.306s`
  - verdict:
    - prioritizing direct/dependent packages did not improve the heavy lane
    - reverted
- add single steady:
  - earlier subset:
    - mg: `~164ms`
    - bun: `~142ms`
    - pnpm: `~846ms`
  - latest subset:
    - mg: `~43.6ms`
    - bun: `~54.1ms`
    - pnpm: `~770ms`
  - verdict: MG now wins this measured subset
- remove single steady:
  - earlier subset:
    - mg: `~74ms`
    - bun: `~62ms`
    - pnpm: `~708ms`
  - latest subset:
    - mg: `~33.3ms`
    - bun: `~75.5ms`
    - pnpm: `~676.6ms`
  - verdict: MG now wins this measured subset
- list:
  - mg: `~105.3ms`
  - bun: `~888.1ms`
  - pnpm: `~2.215s`
  - verdict: MG wins
- build:
  - mg: `~147.9ms`
  - bun: `~1.386s`
  - pnpm: `~2.666s`
  - verdict: MG wins
- `mg create-web`:
  - mg: `~1.868s`
  - verdict: MG-only lane, healthy baseline for scaffold path

Important reading rule:

- a failed local benchmark from `2026-07-27 20:31` was caused by sandboxed registry access, not by a valid apples-to-apples runtime result
- use the later networked run at `2026-07-27 20:32` as the current trustworthy subset snapshot
- use the networked run at `2026-07-27 20:38` as the newer cold/warm install snapshot after the strict-layout prefetch-wait removal
- another failed local heavy run on `2026-07-27 22:36` had the same class of problem:
  - `benchmark_brutal_results_20260727_223632.md`
  - root cause: sandboxed registry access during `heavy-empty-cache-install`
  - direct networked reproduce of `mg install --core web --ignore-scripts` on `heavy-web` completed successfully in about `13.5s`
  - benchmark rerun with real network access:
    - `benchmark_brutal_results_20260727_223755.md`
    - mg: `~21.583s`
  - reading rule:
    - direct install timing is closer to engine cost
    - full benchmark timing includes fixture/bootstrap/harness overhead

## Remaining blockers before stronger product claims

### 1. Cold online path is still the biggest performance debt

Steady/cached paths are much better now, but cold install is still materially behind the target.  
The main remaining debt is install pipeline shape, not CLI polish:

- resolver metadata work is still expensive
- install still has large queue-wait behavior in cold download phases
- some prefetch strategies helped warm paths but hurt cold paths when placed on the critical path

### 2. CLI surface honesty is better, but not fully finished

Still needs another pass for:

- `self-update` real implementation
- full `--recursive` engine instead of a guarded refusal
- remaining non-web core commands are intentionally not implemented

### 3. Product security posture is improved by refusal, not yet by full capability

Current state is better than faking success:

- unsupported audit/core paths refuse to lie
- unsupported recursive flow refuses to lie
- web install path can now enforce a real strict gate for 24h publish freshness plus advisory blocking

But still missing for a stronger product promise:

- deeper tarball integrity enforcement audit
- broader cache poisoning / stale metadata / corrupted shared-cache scenarios
- full benchmarked security comparison against pnpm-like expectations

## Recommendation

Ship as:

- **core-web beta**
- **not** “Bun killer”
- **not** “pnpm replacement”

Correct marketing posture right now:

- native Rust-first core-web beta
- strong scaffold surface
- fast steady/cached operations
- active work on cold-path install performance and cache/integrity hardening

## Next technical priorities

1. streamed or more parallel cold install materialization
2. resolver metadata cold-path reduction
3. stronger cache/tarball integrity verification
4. real recursive workspace engine
5. real self-update flow
