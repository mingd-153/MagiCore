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
- on **Tuesday, July 28, 2026 14:18-14:21**, the tarball-prefetch strategy matrix was finally measured directly:
  - code change:
    - `adapters/web/src/lib.rs`
    - added:
      - `MEGAGATE_WEB_BATCH_PREFETCH`
      - `MEGAGATE_WEB_BATCH_PREFETCH_CONCURRENCY`
    - initial default changed to:
      - batch prefetch: `off`
      - resolve prefetch: `on`
  - verification:
    - `cargo test -p mg-web-adapter --lib` -> `50/50`
    - `cargo test -p mg-resolver` -> `16/16`
  - matrix (`heavy-empty-cache-install-direct`, single-run each):
    - baseline:
      - batch `on(8)`, resolve `off`
      - `benchmark_brutal_results_20260728_141819.md`
      - mg: `~23.508s`
    - batch24:
      - batch `on(24)`, resolve `off`
      - `benchmark_brutal_results_20260728_141909.md`
      - mg: `~23.016s`
    - resolve24:
      - batch `off`, resolve `on`
      - `benchmark_brutal_results_20260728_141938.md`
      - mg: `~19.089s`
    - both24:
      - batch `on(24)`, resolve `on`
      - `benchmark_brutal_results_20260728_142001.md`
      - mg: `~27.975s`
    - both off:
      - `benchmark_brutal_results_20260728_142058.md`
      - mg: `~21.026s`
    - default verify after code change:
      - `benchmark_brutal_results_20260728_142157.md`
      - mg: `~21.607s`
  - verdict:
    - `resolve prefetch on / batch prefetch off` was the best single-run result
    - `both on` was the worst strategy
    - that was enough to justify more testing, but not enough to call the default proven
- on **Tuesday, July 28, 2026 14:25-14:32**, the same lane was rerun with `N=3` to reduce benchmark noise:
  - baseline:
    - `benchmark_brutal_results_20260728_142546.md`
    - mg: `~36.120s ± 3.471s`
  - batch24:
    - `benchmark_brutal_results_20260728_142740.md`
    - mg: `~50.068s ± 4.707s`
  - resolve24:
    - `benchmark_brutal_results_20260728_143016.md`
    - mg: `~33.940s ± 2.763s`
  - both off:
    - `benchmark_brutal_results_20260728_143203.md`
    - mg: `~26.977s ± 2.874s`
  - verdict:
    - the single-run winner did not hold under `N=3`
    - the best measured configuration in this rerun is now:
      - `batch prefetch off`
      - `resolve prefetch off`
    - `resolve prefetch on / batch prefetch off` is still better than both batch-prefetch variants, but it is no longer the winner
    - honest beta conclusion:
      - keep batch-prefetch disabled
      - do not treat resolve-prefetch-enabled as a proven default yet
      - revert the beta default to `batch off / resolve off`
- on **Tuesday, July 28, 2026 14:35**, the beta default was moved back to `batch off / resolve off` and spot-verified on the current binary:
  - verification:
    - `cargo test -p mg-web-adapter --lib` -> `50/50`
    - `cargo test -p mg-resolver` -> `16/16`
    - `benchmark_brutal_results_20260728_143502.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~18.716s`
  - verdict:
    - promising single-run
    - still not enough to replace the `N=3` matrix as the source of truth
    - acceptable beta default because it aligns with the current best repeated measurement
- on **Tuesday, July 28, 2026 14:40**, the prefetch-policy default was also locked in with tests:
  - added:
    - `test_prefetch_defaults_are_conservative`
    - `test_prefetch_flags_can_be_enabled_explicitly`
  - verification:
    - `cargo test -p mg-web-adapter --lib` -> `52/52`
  - verdict:
    - this does not improve raw speed by itself
    - it does reduce the chance of accidental policy drift while cold-path tuning continues
- on **Tuesday, July 28, 2026 14:45**, speculative tarball cache writes were tightened to match install-path integrity behavior:
  - change:
    - added `prepare_verified_tarball_for_cache(...)`
    - reused it in:
      - strict install download path
      - `on_batch_resolved(...)`
      - `spawn_tarball_download(...)`
  - verification:
    - `cargo test -p mg-web-adapter --lib` -> `52/52`
  - verdict:
    - this is a correctness and cache-hygiene improvement
    - it reduces the chance of polluted shared tarball cache entries
    - it is not, by itself, evidence of a cold-path speed improvement
- on **Tuesday, July 28, 2026 14:43-14:44**, resolver metadata batch paths were simplified:
  - change:
    - `prefetch_versions(...)` and `prefetch_dependencies(...)` now reuse `prefetch_resolution_metadata(...)`
    - this removes duplicated future-building and repeated metadata lock/cache-check paths
  - verification:
    - `cargo test -p mg-web-adapter --lib` -> `52/52`
    - `cargo test -p mg-resolver` -> `16/16`
  - benchmark spot-check:
    - `benchmark_brutal_results_20260728_144313.md`
    - `heavy-empty-cache-install-direct`
      - mg: `~20.286s`
  - verdict:
    - promotable as a code-path cleanup
    - not promotable as a performance claim yet
- on **Tuesday, July 28, 2026 14:46**, alias metadata prefetch dedupe was tightened further:
  - change:
    - `prefetch_resolution_metadata(...)` now dedupes by source package, not alias name
    - fetched metadata is then fanned back out to each alias key
  - verification:
    - added `test_prefetch_resolution_metadata_dedupes_aliases_by_source_package`
    - `cargo test -p mg-web-adapter --lib` -> `53/53`
  - verdict:
    - this removes a concrete class of duplicate metadata fetches
    - especially relevant for alias-heavy graphs
    - still not benchmark-quantified enough to market as a speed win yet
- on **Tuesday, July 28, 2026 14:49-14:53**, the new alias-heavy benchmark lane exposed and then validated a real root-alias correctness bug:
  - initial failure:
    - root alias packages were being prefetched/resolved as if the alias name itself were the registry package
    - examples observed during reproduce:
      - `strip-ansi-a` 404 metadata fetch
      - `no version of 'lodash-e' matches 'npm:lodash@^4.17.21'`
  - fix:
    - in `resolve(...)`, root manifest alias dependencies now:
      - register alias target mappings before solver work begins
      - normalize `npm:<target>@<range>` down to the actual range passed into the solver
  - verification:
    - direct reproduce install now succeeds:
      - `32 packages installed`
      - `3016 ms total`
    - benchmark lane now succeeds:
      - `benchmark_brutal_results_20260728_145320.md`
      - `alias-heavy-empty-cache-install-direct`
      - mg: `~4.972s`
  - verdict:
    - promotable and important
    - this is a correctness fix first, benchmarkability fix second
    - alias-heavy is now a real measurable lane instead of a permanent failure case
- on **Tuesday, July 28, 2026 14:55**, a direct profiled run of the heavy cold lane clarified the remaining bottleneck:
  - observed:
    - `resolve_graph=8517ms`
    - `adapter_install=13289ms`
    - `prepare_extracted_roots=13147ms`
    - pipeline:
      - `packages=642`
      - `bytes=104468299`
      - `download_ms_total=209611`
      - `extract_ms_total=5255`
  - verdict:
    - the remaining product blocker is not lockfile/prune tail work
    - it is still:
      - resolver/metadata cost
      - download-dominated install preparation
    - future optimization should target those two regions first
- on **Tuesday, July 28, 2026 14:58**, one concrete disk-churn cut in the strict cold path did produce a measurable improvement:
  - change:
    - when shared cache exists, strict installs no longer double-write tarballs through local cache on the cold path
    - shared cache is treated as the canonical tarball store for that path
  - verification:
    - `cargo test -p mg-web-adapter --lib` -> `53/53`
  - benchmark:
    - before:
      - `benchmark_brutal_results_20260728_144313.md`
      - `heavy-empty-cache-install-direct`
      - mg: `~20.286s`
    - after:
      - `benchmark_brutal_results_20260728_145849.md`
      - `heavy-empty-cache-install-direct`
      - mg: `~19.286s`
  - verdict:
    - small but real improvement
    - not enough to close the broader cold-path gap by itself
    - good evidence that duplicated tarball cache I/O was worth removing
- on **Tuesday, July 28, 2026 15:09**, overlapping next-wave resolver prefetch produced a larger follow-up win:
  - change:
    - after `prefetch_dependencies(...)`, the resolver now prefetches versions for the next dependency wave
    - that prefetch is overlapped with `add_resolution(...)` using `futures::join(...)`
  - verification:
    - `cargo test -p mg-resolver` -> `16/16`
    - `cargo test -p mg-web-adapter --lib` -> `53/53`
  - resolver profile:
    - before:
      - `solve_total=10357ms`
    - after:
      - `solve_total=6362ms`
  - end-to-end benchmark:
    - before:
      - `benchmark_brutal_results_20260728_145849.md`
      - `heavy-empty-cache-install-direct`
      - mg: `~19.286s`
    - after:
      - `benchmark_brutal_results_20260728_150927.md`
      - `heavy-empty-cache-install-direct`
      - mg: `~18.443s`
  - verdict:
    - this is a meaningful resolver-path win
    - it does not finish the cold-path story, but it materially improves the most expensive lane
- on **Tuesday, July 28, 2026 15:12-15:13**, the heavy cold lane was re-tuned again on the new baseline:
  - sweep results:
    - `download_concurrency=16`
      - `benchmark_brutal_results_20260728_151212.md`
      - mg: `~19.150s`
    - `download_concurrency=24`
      - `benchmark_brutal_results_20260728_151235.md`
      - mg: `~17.450s`
    - `download_concurrency=32`
      - `benchmark_brutal_results_20260728_151257.md`
      - mg: `~19.270s`
  - verdict:
    - `24` remains the best measured value
    - the best measured heavy cold-lane result so far is now `~17.450s`
    - this is still not “faster than Bun” evidence by itself, but it is a materially better beta baseline
- on **Tuesday, July 28, 2026 15:16-15:17**, pipeline task concurrency was re-tested on the new baseline and rejected again:
  - sweep:
    - `pipeline_task_concurrency=24` -> `~20.326s`
    - `pipeline_task_concurrency=56` -> `~20.674s`
    - `pipeline_task_concurrency=80` -> `~23.851s`
  - compare against kept baseline:
    - `heavy-empty-cache-install-direct` -> `~17.450s`
  - verdict:
    - still non-promotable
    - keep the default behavior
    - do not spend more time on this knob until a stronger hypothesis appears
- on **Tuesday, July 28, 2026 15:19-15:21**, metadata concurrency was re-tested on the new baseline:
  - sweep:
    - `16` -> `~19.902s`
    - `24` -> `~20.614s`
    - `32` -> `~18.264s`
  - confirmation run for `32`:
    - `~19.686s`
  - verdict:
    - there may be signal here, but it is not stable enough yet
    - do not promote a new default from this evidence alone
- on **Tuesday, July 28, 2026 15:25**, the heavy cold baseline was re-run with `N=3`:
  - result:
    - `benchmark_brutal_results_20260728_152537.md`
    - mg:
      - mean `~20.413s`
      - sigma `~2.092s`
      - range `18.023s .. 21.913s`
  - verdict:
    - the recent improvements are real enough to show up in repeated measurement
    - but the lane still has substantial variance
    - use this repeated result, not the prettiest single run, as the more honest beta baseline
- on **Tuesday, July 28, 2026 15:30-15:31**, optional enqueue memoization was kept, but resolve-prefetch was re-checked and rejected again on the current binary:
  - code:
    - `NpmDependencyProvider::should_enqueue(...)` now memoizes optional package support decisions in `optional_enqueue_cache`
  - verify:
    - `cargo test -p mg-resolver` -> `16/16`
    - `cargo test -p mg-web-adapter --lib` -> `53/53`
  - benchmark with current default (`resolve prefetch off`):
    - `benchmark_brutal_results_20260728_153044.md`
    - `heavy-empty-cache-install-direct` -> `~20.201s`
  - benchmark with `MEGAGATE_WEB_RESOLVE_PREFETCH=1` forced back on:
    - `benchmark_brutal_results_20260728_153148.md`
    - `heavy-empty-cache-install-direct` -> `~24.720s`
  - reading rule:
    - even if `resolve_graph` alone looks a little better with resolve-prefetch enabled, total cold install becomes much worse
    - the overlap is still increasing downstream install pressure more than it helps the front half
  - verdict:
    - keep `resolve prefetch` default off
    - keep optional enqueue memoization as a safe local cleanup
    - do not promote this round as a cold-path breakthrough
- on **Tuesday, July 28, 2026 15:35**, metadata cache cloning pressure was reduced by switching the hot metadata cache to `Arc<PackageMetadata>`:
  - code:
    - `MetadataCache` now stores `Arc<PackageMetadata>`
    - `metadata(...)`, `prefetch_resolution_metadata(...)`, and metadata fallback loading were updated to stay on shared metadata references longer
  - verify:
    - `cargo test -p mg-resolver` -> `16/16`
    - `cargo test -p mg-web-adapter --lib` -> `53/53`
  - benchmark:
    - `benchmark_brutal_results_20260728_153516.md`
    - `heavy-empty-cache-install-direct` -> `~20.551s`
  - reading rule:
    - this is a clean hot-path memory/copy reduction
    - it did not produce a decisive cold-lane wall-clock win by itself
    - resolve stayed in a healthy range, but install/materialization still dominates the total
  - verdict:
    - keep the `Arc` conversion
    - do not market it as a breakthrough
    - continue product work on first-run metadata scheduling and install-phase pressure
- on **Tuesday, July 28, 2026 15:36-15:37**, tighter pipeline task limits were re-tested on the current binary and failed badly again:
  - config:
    - lane: `heavy-empty-cache-install-direct`
    - `download_concurrency=24`
  - results:
    - `pipeline_task_concurrency=24`
      - `benchmark_brutal_results_20260728_153647.md`
      - `~33.218s`
    - `pipeline_task_concurrency=32`
      - `benchmark_brutal_results_20260728_153651.md`
      - `~30.754s`
  - compare against current default-band runs:
    - `benchmark_brutal_results_20260728_153044.md` -> `~20.201s`
    - `benchmark_brutal_results_20260728_153516.md` -> `~20.551s`
  - reading rule:
    - this is not noise
    - both tighter settings materially degraded the lane
    - the hypothesis that “less pipeline task pressure might help now” remains false on the current binary
  - verdict:
    - keep current default behavior
    - stop tuning this knob for now
- on **Tuesday, July 28, 2026 15:43**, resolver-side version-batch copy pressure was reduced and the heavy cold lane did improve:
  - code:
    - `prefetched_versions` in the resolver now stores `Arc<[Version]>`
    - select paths reuse prefetched version slices instead of cloning whole version vectors repeatedly
  - verify:
    - `cargo test -p mg-resolver` -> `16/16`
    - `cargo test -p mg-web-adapter --lib` -> `53/53`
  - benchmark:
    - `benchmark_brutal_results_20260728_154349.md`
    - `heavy-empty-cache-install-direct` -> `~19.278s`
  - reading rule:
    - this is a real positive spot-run
    - it is still a spot-run, not yet a replacement for the more honest repeated baseline
    - install/materialization still dominates the total, but the resolver-side copy reduction looks valid
  - verdict:
    - keep the resolver `Arc<[Version]>` change
    - re-run the heavy lane with `N=3` before turning this into a stronger claim
- on **Tuesday, July 28, 2026 15:47**, the `N=3` re-check invalidated the pretty `~19.278s` spot-run as a trustworthy baseline:
  - result:
    - `benchmark_brutal_results_20260728_154729.md`
    - `heavy-empty-cache-install-direct`
    - mean `~23.487s`
    - sigma `~0.316s`
    - range `23.127s .. 23.714s`
  - compare:
    - earlier spot-run:
      - `benchmark_brutal_results_20260728_154349.md`
      - `~19.278s`
  - reading rule:
    - keep the resolver `Arc<[Version]>` cleanup
    - do not treat the `~19.278s` run as a stable new cold baseline
    - the repeated lane is still materially slower than that spot-run, which means materialization/network pressure still dominates
  - verdict:
    - honest baseline remains in the `~23.5s` band for this exact repeated setup
    - next optimization focus should move toward strict-layout materialization and download-to-materialize pressure
- on **Tuesday, July 28, 2026 15:53**, a follow-up dependency-side resolver copy-reduction experiment was verified as safe but rejected on performance:
  - code:
    - dependency prefetch results in the resolver were changed to `Arc<[ResolvedDep]>`
    - `add_resolution(...)` now reads dependency slices instead of cloning dependency vectors per selected package
  - verify:
    - `cargo test -p mg-resolver` -> `16/16`
    - `cargo test -p mg-web-adapter --lib` -> `53/53`
  - benchmark:
    - `benchmark_brutal_results_20260728_155321.md`
    - `heavy-empty-cache-install-direct` -> `~24.787s`
  - reading rule:
    - correctness stayed clean
    - performance did not improve; this run landed worse than the current repeated baseline band
  - verdict:
    - do not keep investing in resolver-side copy micro-tuning right now
    - move the next optimization loop toward strict-layout materialization and download-to-materialize pressure
- on **Tuesday, July 28, 2026 15:57**, overlapping shared tarball-cache persistence with extraction produced a strong cold-lane spot-run:
  - code:
    - `TarballFetchResult.bytes` now uses `Arc<[u8]>`
    - on the strict/shared-cache path, shared tarball persistence is no longer completed synchronously before extraction starts
    - shared-cache warming now overlaps with extraction work
  - verify:
    - `cargo test -p mg-resolver` -> `16/16`
    - `cargo test -p mg-web-adapter --lib` -> `53/53`
  - benchmark:
    - `benchmark_brutal_results_20260728_155700.md`
    - `heavy-empty-cache-install-direct` -> `~18.792s`
  - reading rule:
    - this is a much more believable spot-run than the recent resolver copy-only experiments because it attacks the cold critical path directly
    - it is still a spot-run, not yet a repeated baseline
  - verdict:
    - keep the change
    - immediately re-run `N=3` before using this number as the new honest baseline
- on **Tuesday, July 28, 2026 15:59**, the `N=3` check confirmed this change helps, but by less than the pretty spot-run suggested:
  - result:
    - `benchmark_brutal_results_20260728_155901.md`
    - `heavy-empty-cache-install-direct`
    - mean `~22.138s`
    - sigma `~1.811s`
    - range `20.079s .. 23.480s`
  - compare:
    - previous repeated baseline:
      - `benchmark_brutal_results_20260728_154729.md`
      - `~23.487s ± 0.316s`
    - earlier spot-run:
      - `benchmark_brutal_results_20260728_155700.md`
      - `~18.792s`
  - reading rule:
    - this is a real repeated improvement over the prior repeated baseline
    - but the repeated gain is materially smaller than the single-run number implied
    - the lane still carries real variance
  - verdict:
    - keep the overlap change
    - use `~22.138s ± 1.811s` as the more honest repeated baseline for this exact setup
    - do not treat `~18.792s` as the production-facing number
- on **Tuesday, July 28, 2026 16:04**, reusing the shared `PackageCache` handle inside the pipeline proved safe but did not move the cold lane enough to matter:
  - code:
    - `pipeline_download_and_extract(...)` now resolves the shared `PackageCache` once
    - shared tarball reads and shared-cache persistence reuse that handle instead of reopening it per package
  - verify:
    - `cargo test -p mg-resolver` -> `16/16`
    - `cargo test -p mg-web-adapter --lib` -> `53/53`
  - benchmark:
    - `benchmark_brutal_results_20260728_160443.md`
    - `heavy-empty-cache-install-direct` -> `~22.258s`
  - reading rule:
    - this is a valid cleanup and removes repeated setup work
    - but the heavy cold lane still does not materially improve from it
  - verdict:
    - keep the cleanup
    - do not treat it as a performance milestone
    - continue pushing into extraction/materialization pressure instead
- on **Tuesday, July 28, 2026 16:13**, moving shared-cache persist waits until after materialization produced another encouraging cold-lane spot-run:
  - code:
    - `pipeline_download_and_extract(...)` now returns persist handles separately
    - strict-layout install no longer awaits shared-cache warming inside each extraction task
    - shared-cache persist joins now happen after materialization instead of during the extraction critical path
  - verify:
    - `cargo test -p mg-resolver` -> `16/16`
    - `cargo test -p mg-web-adapter --lib` -> `53/53`
  - benchmark:
    - `benchmark_brutal_results_20260728_161356.md`
    - `heavy-empty-cache-install-direct` -> `~21.397s`
  - reading rule:
    - this is another positive spot-run in the same strict-layout optimization family
    - it still does not replace the current repeated baseline by itself
  - verdict:
    - keep the change
    - continue using the repeated baseline as the honest product-readiness number until a new `N=3` confirms the shift
- on **Tuesday, July 28, 2026 16:18**, the `N=3` re-check confirmed that the strict-layout line really did move the cold baseline down:
  - result:
    - `benchmark_brutal_results_20260728_161833.md`
    - `heavy-empty-cache-install-direct`
    - mean `~19.538s`
    - sigma `~1.819s`
    - range `18.487s .. 21.639s`
  - compare:
    - previous repeated baseline:
      - `benchmark_brutal_results_20260728_155901.md`
      - `~22.138s ± 1.811s`
  - reading rule:
    - this is a real repeated improvement, not just a pretty spot-run
    - the lane still has variance and even emitted an outlier warning
    - but the center of the repeated baseline has moved down materially
  - verdict:
    - keep the strict-layout cold-path changes from this sequence
    - use `~19.538s ± 1.819s` as the new more honest repeated baseline for this exact setup
    - still do not call the product ready yet from this number alone
- on **Tuesday, July 28, 2026 16:21**, a follow-up single cold-lane run spiked badly and should be treated as an outlier, not as the new baseline:
  - result:
    - `benchmark_brutal_results_20260728_162142.md`
    - `heavy-empty-cache-install-direct` -> `~38.176s`
  - reading rule:
    - this run sat far outside the current repeated baseline band
    - the profile showed a broad network/download spike, not a neat isolated regression
    - it is evidence that the lane is still noise-sensitive, not evidence that the repeated baseline from `16:18` is invalid
  - verdict:
    - keep using `~19.538s ± 1.819s` as the more honest repeated baseline for this strict-layout line
    - treat this run as an outlier unless a new repeated benchmark reproduces the regression
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

## Tuesday, July 28, 2026 16:26 update

- mot regression correctness da xuat hien trong strict layout:
  - toi uu lay `parent()` tu `strict_vstore_package_dir(...)` lam sai layout voi package scoped nhu `@nuxt/kit`
  - nested dependency links bi dat nham vao `.../node_modules/@scope` thay vi `.../node_modules`
- da sua bang cach:
  - tach `strict_vstore_node_modules_dir(...)`
  - dung helper nay cho dependency-link phase
  - giu lai cac toi uu strict-path an toan:
    - overlap shared-cache persist voi extraction
    - doi shared-cache persist wait toi sau materialization
    - bo qua recreate symlink neu target da dung
- verification:
  - `cargo test -p mg-web-adapter --lib`: `53 passed`
  - `cargo test -p mg-resolver`: `16 passed`
- benchmark moi nhat:
  - `benchmark_brutal_results_20260728_162642.md`
  - lane `heavy-empty-cache-install-direct`
  - mg: `~21.995s +- 1.390s`
  - range: `20.398s .. 22.938s`
- honest read:
  - run moi nay khong tot hon repeated baseline manh nhat hien tai `~19.538s +- 1.819s`
  - nghia la correctness da ve xanh, nhung cold heavy lane chua co them breakthrough moi
  - vi vay trang thai beta-readiness khong doi:
    - co the tiep tuc ship nhu `core-web beta`
    - chua du de claim Bun/pnpm-class cold leadership

## Tuesday, July 28, 2026 16:40 update

- product surface da duoc siet lai de phu hop voi thuc te release:
  - `available_cores()` chi con cong bo `web`
  - help surface khong con lo cac lenh `create-game`, `add-ai`, `install-app`, ... nua
  - cac core khac van la stub noi bo, nhung khong con bi trinh bay nhu feature san sang cho user
- verification:
  - `cargo test -p mg test_available_cores_matches_build_shape -- --nocapture`
  - `cargo test -p mg test_help_surface_matches_build_shape -- --nocapture`
  - pass
- tac dong:
  - tu day co the goi ban nay la `core-web beta` trung thuc hon
  - no khong bien non-web cores thanh implemented
  - nhung no cat mot no ky thuat quan trong o lop CLI/product messaging

## Tuesday, July 28, 2026 16:55 update

- strict install path da duoc cat bot them mot fs churn thua:
  - `staging_root/node_modules` truoc day van duoc tao cho moi install
  - nhung nhanh strict layout khong he su dung no
- da sua:
  - chi tao `staging_root` khi di vao `legacy_flat`
  - strict path khong con tao/xoa staging tree vo nghia nua
- verification:
  - `cargo test -p mg-web-adapter --lib`: `53 passed`
  - `cargo test -p mg-resolver`: `16 passed`
- benchmark moi:
  - `benchmark_brutal_results_20260728_165509.md`
  - lane `heavy-empty-cache-install-direct`
  - mg: `~19.759s +- 1.201s`
  - range: `18.459s .. 20.828s`
- honest read:
  - ket qua nay tot hon run `~21.995s +- 1.390s` ngay truoc do
  - no dua heavy cold lane tro lai gan repeated band manh hon cua strict-path line
  - nhung van chua du de claim cold leadership truoc Bun/pnpm

## Tuesday, July 28, 2026 17:20 update

- them mot cum I/O nen da duoc cat khoi strict install path:
  - nhanh strict truoc day van mo `store.db` va ghi `insert_package(...)`
  - nhung dependency graph strict hien tai khong can DB nay de materialize
- da sua:
  - chi mo DB khi `legacy_flat == true`
  - strict install path khong con mo/ghi DB vo nghia nua
- verification:
  - `cargo test -p mg-web-adapter --lib`: `53 passed`
  - `cargo test -p mg-resolver`: `16 passed`
- benchmark moi:
  - `benchmark_brutal_results_20260728_172050.md`
  - lane `heavy-empty-cache-install-direct`
  - mg: `~18.711s +- 1.797s`
  - range: `17.287s .. 20.730s`
- honest read:
  - ket qua nay tot hon run `~19.759s +- 1.201s` truoc do
  - no cung tot hon repeated baseline `~19.538s +- 1.819s` da giu tu truoc
  - chua du de ket luan cold path da dat muc Bun/pnpm-class
  - nhung day la mot buoc tien co do duoc va dang nen giu
