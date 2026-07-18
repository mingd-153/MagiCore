# Core Web Performance Audit - 2026-07-18

Scope: `core-web`, benchmark harness, install/add/remove/dev paths, and native backend runtime lanes.

## What changed

- Replaced the placeholder `mg dev` web server path with the real core-web dev launcher.
  - `mg dev --core web --host 127.0.0.1 --port <port>` now delegates to the same framework/native launcher used by `mg dev-web`.
- Fixed benchmark dev probing.
  - The old probe could block while reading streaming dev-server responses.
  - The probe now only reads a small body when `Content-Length` is present.
  - It rejects the old placeholder response string.
  - It kills the whole spawned process tree so Vite/Maven/etc. children do not keep ports alive.
- Added native backend benchmark lanes:
  - `backend-go-echo`
  - `backend-rust-axum`
  - `backend-python-fastapi`
  - `backend-java-spring`
  - Each lane runs `mg create-web -> mg install-web -> mg dev` and probes `/api/health`.
- Fixed Spring Boot dev launcher.
  - Old command passed `--server.port=4324,--server.address=127.0.0.1`.
  - Spring parsed that as an invalid integer port.
  - New command passes `--server.port=4324 --server.address=127.0.0.1`.
- Added command-level install profiling.
  - `MEGAGATE_WEB_PROFILE_INSTALL=1` now prints both command-level phases and adapter install phases.
- Increased default npm metadata freshness TTL.
  - Old default: `300s`.
  - New default: `21600s` / 6 hours.
  - `MEGAGATE_WEB_METADATA_TTL_SECS=0` can force immediate metadata revalidation.
- Added `mg remove` lock-prune fast path.
  - After removing a dependency, if the existing `mg.lock` still covers all remaining direct dependencies, MegaGate prunes the old graph instead of resolving the full registry graph again.
  - Fallback remains the old full resolve path if lock coverage is insufficient.

## Verified commands

- `cargo check -p mg`
- `cargo test -p mg-web-adapter stale_metadata -- --nocapture`
- `cargo test -p mg lock -- --nocapture`
- `cargo test -p mg test_build_dev_launch_supports_native_symfony_and_quarkus_backends -- --nocapture`
- `cargo build --release -p mg`
- `cargo test -p mg-resolver`
- `cargo test -p mg-web-adapter`
- `cargo test -p mg no_install -- --nocapture`
- `cargo test -p mg --test web_cli_surface -- --nocapture`

## Benchmark outputs

Generated reports:

- `benchmark_brutal_results_20260718_163000.md`
- `benchmark_brutal_results_20260718_163336.md`
- `benchmark_brutal_results_20260718_163352.md`
- `benchmark_brutal_results_20260718_163632.md`
- `benchmark_brutal_results_20260718_164408.md`
- `benchmark_brutal_results_20260718_164624.md`
- `benchmark_brutal_results_20260718_164644.md`
- `benchmark_brutal_results_20260718_165845.md`
- `benchmark_brutal_results_20260718_165920.md`
- `benchmark_brutal_results_20260718_170030.md`

## Update: stricter benchmark and correctness cleanup

### CLI/package mutation split

Added manifest-only mutation support:

- `mg add ... --no-install`
- `mg add-web ... --no-install`
- `mg remove ... --no-install`
- `mg remove-web ... --no-install`

This separates:

- package manifest mutation
- lockfile/node_modules materialization
- full add/remove + install behavior

Real result from `benchmark_brutal_results_20260718_164624.md`:

| Lane | MG | Bun | pnpm | Read |
| --- | ---: | ---: | ---: | --- |
| `add-single-mutate-only` | `28.2ms` | `114.5ms` | `914.4ms` | MG fastest on manifest-only mutation |
| `remove-single-mutate-only` | `18.4ms` | `27.2ms` | `809.0ms` | MG fastest on manifest-only mutation |

Important caveat:

- Bun lane uses `--dry-run`, so it is not a perfect semantic match for MG because MG mutates `package.json`.
- pnpm lane uses `--lockfile-only`, because pnpm v11 does not expose a direct dry-run equivalent.

### Empty shared-cache benchmark

Added isolated-cache lanes:

- `empty-cache-install`
- `heavy-empty-cache-install`

These clear MegaGate shared cache, Bun cache, pnpm store, npm cache, and Yarn cache inside the benchmark workdir.

Best observed MG improvement after abbreviated npm metadata:

| Report | Lane | MG |
| --- | --- | ---: |
| `benchmark_brutal_results_20260718_165845.md` | `empty-cache-install` | `18.057s` |

Direct comparison from `benchmark_brutal_results_20260718_165920.md`:

| Lane | MG | Bun | pnpm | Winner |
| --- | ---: | ---: | ---: | --- |
| `empty-cache-install` | `20.406s` | `6.484s` | `4.609s` | pnpm |

Current honest read:

- MG improved from roughly `35s` to roughly `18-20s` on isolated empty-cache install after requesting abbreviated npm metadata.
- MG is still about `4.4x` slower than pnpm and about `3.1x` slower than Bun in this lane.
- This is not product-competitive for cold online install yet.

### Resolver/fetcher optimizations added

- Metadata requests now use `Accept: application/vnd.npm.install-v1+json`.
- Non-optional dependency enqueue avoids platform metadata checks.
- Provider prefetch checks in-memory metadata before disk/network reload.
- Resolver batch size is configurable through `MEGAGATE_RESOLVER_BATCH_SIZE` and defaults to `250`.

Result:

- Abbreviated metadata materially helped.
- Batch-size tuning did not materially improve the small empty-cache graph.
- The remaining cold bottleneck is still resolver metadata traffic plus tarball/materialization work.

### Native backend benchmark with native baselines

Command:

```bash
BENCH_LANES=backend-go-echo,native-go-echo-baseline,backend-rust-axum,native-rust-axum-baseline,backend-python-fastapi,native-python-fastapi-baseline,backend-java-spring,native-java-spring-baseline \
BENCH_PMS=mg \
BENCH_RUNS=1 \
BENCH_WARMUP=0 \
BACKEND_TIMEOUT_SECONDS=120 \
bash benchmark.sh
```

Result from `benchmark_brutal_results_20260718_170030.md`:

| Lane | Result | Time |
| --- | --- | ---: |
| `backend-go-echo` | Pass | `1.858s` |
| `native-go-echo-baseline` | Pass | `647.1ms` |
| `backend-rust-axum` | Pass | `12.985s` |
| `native-rust-axum-baseline` | Pass | `12.596s` |
| `backend-python-fastapi` | Pass | `13.645s` |
| `native-python-fastapi-baseline` | Pass | `15.032s` |
| `backend-java-spring` | Pass | `5.027s` |
| `native-java-spring-baseline` | Pass | `4.901s` |

Read:

- All native backend lanes pass.
- Rust and Java are close to native baseline.
- Python is slightly faster in this run, likely because both lanes reuse local pip cache and variance matters.
- Go has visible MegaGate overhead because the MG lane includes create/install/dev orchestration.
- These lanes prove backend templates can run, not that MegaGate has replaced each ecosystem package manager.

### Anti-fake cleanup

- `mg self-update` now refuses to fake version checks or pretend to update.
- `mg start` binds to `127.0.0.1` by default.
- `mg dev` defaults to `127.0.0.1`.
- `link` package-source lookup no longer calls `npm root -g`.
- Registry HTTP user-agent no longer hardcodes `MegaGate/0.1.0`; it uses the build crate version.
- Core-web scaffold fallback versions were moved out of Rust logic into `templates/web/versions/scaffold-baseline.toml`.
- Rust still parses and validates that TOML at build/test time through `include_str!`, so broken version metadata fails loudly.

Remaining suspicious areas from code scan:

- Template deployment files still include ecosystem commands like `npm run build`, `npm run dev`, and `npm install` for Vercel/Railway/GitHub Actions/Dockerfile compatibility.
- That is acceptable only as output compatibility for external platforms. It must not be used by MegaGate internal install/dev/build paths.
- Several generated frontend testing configs still use default framework assumptions, for example Cypress component config says React in some non-React templates. This is a template-quality debt, not a resolver speed debt.

## Native backend benchmark

Command:

```bash
BENCH_LANES=backend-go-echo,backend-rust-axum,backend-python-fastapi,backend-java-spring \
BENCH_PMS=mg \
BENCH_RUNS=1 \
BENCH_WARMUP=0 \
BACKEND_TIMEOUT_SECONDS=120 \
bash benchmark.sh
```

Result:

| Lane | Result | Time |
| --- | --- | ---: |
| `backend-go-echo` | Pass | `1.752s` |
| `backend-rust-axum` | Pass | `13.864s` |
| `backend-python-fastapi` | Pass | `14.430s` |
| `backend-java-spring` | Pass | `5.055s` |

Notes:

- These are native runtime lanes, not npm-style dependency manager comparisons.
- Go/Rust/Python/Java are now actually measured by `benchmark.sh`.
- Rust and Python are still slow on first real project setup because they perform native ecosystem bootstrapping (`cargo fetch`, `.venv + pip install`).
- Java Spring was failing before this pass; it now passes.

## JS/TS install benchmark after cache freshness fix

Command:

```bash
BENCH_LANES=cold-install,heavy-cold-install,add-single,remove-single \
BENCH_PMS=mg,bun,pnpm \
BENCH_RUNS=1 \
BENCH_WARMUP=0 \
bash benchmark.sh
```

Result:

| Lane | MG | Bun | pnpm | Current read |
| --- | ---: | ---: | ---: | --- |
| `cold-install` | `309.9ms` | `1.106s` | `1.876s` | MG faster on this cached metadata/shared-cache run |
| `heavy-cold-install` | `1.175s` | `1.416s` | `3.607s` | MG slightly faster than Bun and faster than pnpm |
| `add-single` | `794.7ms` | `431.9ms` | `3.098s` | MG still slower than Bun |
| `remove-single` | `626.4ms` | `303.5ms` | `2.521s` | MG still slower than Bun |

Important correction:

- The earlier `~11s` small install and `~25s` heavy install were not adapter materialization cost.
- Profiling showed adapter install was only hundreds of milliseconds.
- The large delay came from metadata freshness policy causing many online metadata revalidations.
- After changing TTL from 5 minutes to 6 hours, MG install is no longer `18x` behind Bun in the tested warm-cache scenario.

## MG internal install profile after fix

Small graph:

| Phase | Time |
| --- | ---: |
| `parse_manifest` | `2ms` |
| `resolve_graph` | `183ms` |
| `adapter_install` | `286ms` |
| command total | `473ms` |
| hyperfine wall | `511.4ms` |

Heavy graph:

| Phase | Time |
| --- | ---: |
| `parse_manifest` | `0ms` |
| `resolve_graph` | `515ms` |
| `adapter_install` | `639ms` |
| command total | `1155ms` |
| hyperfine wall | `1.213s` |

## Still not product-ready

- `add` still resolves/installs twice in the benchmark lane: install baseline, mutate manifest, then reinstall.
- `remove` now prunes from lock for the second install, but still rematerializes the graph to keep `mg.lock` and `node_modules` correct.
- True cold online performance from an empty shared cache still needs a separate measured lane.
- Native backend benchmark exists now, but there is no Bun/pnpm-style baseline for Go/Rust/Python/Java because those ecosystems do not map cleanly to npm package manager commands.
- Cross-platform installability is not verified here:
  - macOS tested on this machine.
  - Windows/Linux installer packaging is still not proven by this benchmark.
- Product-level security claims still need dedicated attack lanes:
  - corrupt tarball reuse
  - stale metadata poisoning
  - lifecycle script policy bypass
  - lockfile tampering
  - path traversal during extraction

## Next engineering targets

1. Add a true empty-cache benchmark lane.
2. Split mutate benchmarks into:
   - `install baseline`
   - `add/remove mutate only`
   - `add/remove mutate + materialize`
3. Add a web-specific incremental add path that resolves only the newly added package subtree when lock coverage is sufficient.
4. Add a web-specific remove materializer that prunes unreachable packages and rewrites lock/node_modules without full graph rematerialization.
5. Add backend language benchmark lanes for:
   - Go: `go mod download` vs `mg install-web`
   - Rust: `cargo fetch` vs `mg install-web`
   - Python: `python -m venv && pip install` vs `mg install-web`
   - Java: `mvn dependency:go-offline` vs `mg install-web`
6. Add security benchmark lanes before any production claim.

## Current verdict

Core-web is much healthier than the failed benchmark suggested, but it is not production-competitive yet.

- Install path with warm/shared cache: now competitive in this run.
- Add/remove: still behind Bun.
- Native backend support: now measured and passing for Go Echo, Rust Axum, Python FastAPI, and Java Spring Boot.
- Production readiness: not yet, because cold online, incremental mutation, security attack lanes, and cross-platform packaging are still incomplete.

## Update 3: cold-path optimization pass

Changes added after the stricter empty-cache benchmark still showed MG losing badly:

- Resolver now consumes the dependency list returned by `prefetch_dependencies` directly.
  - Before: the resolver prefetched deps, then called `get_dependencies` again for each selected package.
  - After: prefetched deps become the source of truth for that batch.
- Optional dependency enqueue checks now run concurrently.
  - This matters for packages with many optional native binaries.
- Core-web skips metadata fetches for clearly incompatible optional native binary packages:
  - `@esbuild/*`
  - `@rollup/rollup-*`
  - `@swc/core-*`
  - `@tailwindcss/oxide-*`
  - `lightningcss-*`
  - `@parcel/watcher-*`
- Extracted package markers no longer compute an extra SHA256 over tarballs that already have SRI integrity.
  - Strict SRI remains enforced.
  - The marker uses the SRI fingerprint when present.

Rollback note:

- A trial fast path using npm's package/spec manifest endpoint was attempted and removed.
- It caused slow retry behavior for semver ranges in this environment, so it was not kept.

Verified:

- `cargo test -p mg-resolver`
- `cargo test -p mg-web-adapter`
- `cargo check -p mg`
- `cargo test -p mg no_install -- --nocapture`
- `cargo build --release -p mg`

Latest MG-only profiled empty-cache result:

| Report | Total | `resolve_graph` | `adapter_install` | Read |
| --- | ---: | ---: | ---: | --- |
| `benchmark_brutal_results_20260718_171709.md` | `23.198s` | `16.080s` | `7.078s` | before this pass |
| `benchmark_brutal_results_20260718_173204.md` | `19.425s` | `11.012s` | `8.368s` | after resolver/optional pass |

Latest PM comparison:

Report: `benchmark_brutal_results_20260718_173339.md`

| Lane | MG | Bun | pnpm | Winner |
| --- | ---: | ---: | ---: | --- |
| `empty-cache-install` | `17.972s` | `8.737s` | `7.433s` | pnpm |

Current read after this pass:

- MG improved materially versus the previous bad cold run.
- MG is still not meeting the original goal.
- MG is still about `2.4x` slower than pnpm and about `2.1x` slower than Bun for empty-cache install in the latest comparison.
- The biggest remaining issue is still resolver metadata plus tarball/materialization cost.

Next required fixes before any product claim:

1. Add a real metadata index/bootstrap strategy for core-web installs.
2. Add lockfile-first install for generated/scaffolded projects.
3. Stream tarball download into extraction/cache instead of download -> cache -> reread -> extract.
4. Add in-flight metadata de-duplication across resolver phases, not only within a batch.
5. Re-run heavy empty-cache and full command matrix after those are done.

## Update 4: shared-cache primary and extract I/O pass

Additional changes:

- Resolver default batch size changed from `250` to `64`.
  - Measured candidates:
    - batch `16`: `22.807s`, `resolve_graph=14.224s`
    - batch `32`: `18.422s`, `resolve_graph=9.580s`
    - batch `64`: `18.056s`, `resolve_graph=9.497s`
  - Batch `64` was the best measured default in this environment.
- `mg-fetcher` now supports extracting tarballs from a generic reader.
- Core-web adapter now extracts from in-memory tarball bytes instead of writing `package.tgz` to temp and reading it again.
- `PackageCache` now supports `cache_tarball_from_path`.
- When a shared cache is available and the project local tarball cache is empty, core-web uses the shared tarball cache as the primary cache.
  - This avoids writing the same tarball to both project cache and shared cache on cold install.
  - Local seeded cache still wins, so offline/local-cache tests keep working.

Verified:

- `cargo test -p mg-fetcher`
- `cargo test -p mg-store`
- `cargo test -p mg-web-adapter`
- `cargo test -p mg-resolver`
- `cargo check -p mg`
- `cargo build --release -p mg`

Latest profiled MG result after shared-primary cache:

Report: `benchmark_brutal_results_20260718_175040.md`

| Lane | Total | `resolve_graph` | `adapter_install` | `prefetch_tarballs` |
| --- | ---: | ---: | ---: | ---: |
| `empty-cache-install` | `17.845s` | `11.107s` | `6.701s` | `3.404s` |

Read:

- Adapter install improved versus the previous profiled run:
  - before: `8.264s`
  - after: `6.701s`
- Resolver remains the dominant bottleneck when registry/network is not favorable.
- A noisy comparison run after these changes produced:
  - MG `54.144s`
  - Bun `18.891s`
  - pnpm `7.104s`
  - This run showed severe registry/network variance, but it still proves MG cold path is less robust than pnpm under bad network conditions.

Current verdict remains unchanged:

- This is still a failed product-readiness target.
- The code is better and less wasteful, but MG still does not consistently compete with Bun/pnpm on cold empty-cache install.
- Next serious fix must be metadata bootstrap/indexing or a lockfile-first path for scaffolded/known templates. More local micro-optimizations will not close the gap alone.

## Update 5: shared resolution graph cache

Additional changes:

- Added a shared resolution graph cache under the core-web shared cache root.
- The cache key is a SHA-256 fingerprint of:
  - normalized registry URL
  - dependency group
  - package name
  - version range
  - dependency flags
- The app/project name is intentionally excluded from the key.
  - Two different projects with the same dependency surface can reuse the same resolved graph.
  - Changing dependency ranges or registry creates a different key.
- `WebAdapter::resolve` now checks this shared graph cache before calling the resolver.
- After a real resolver pass succeeds, the resolved graph is written back to shared cache.
- Corrupt or incompatible graph cache files are ignored or removed; the adapter falls back to real resolution.
- Added tests for:
  - offline resolve from shared graph cache
  - deterministic cache key independent of dependency order and app name

Verified:

- `cargo test -p mg-web-adapter`
- `cargo test -p mg-resolver`
- `cargo test -p mg-store`
- `cargo test -p mg-fetcher`
- `cargo check -p mg`
- `cargo build --release -p mg`

Benchmark report:

- `benchmark_brutal_results_20260718_180042.md`
- `benchmark_brutal_results_20260718_180042.json`
- `benchmark_brutal_results_20260718_180042.status.tsv`

All selected benchmark lanes passed:

| Lane | MG | Bun | pnpm | Current read |
| --- | ---: | ---: | ---: | --- |
| `heavy-empty-cache-install` | `47.522s` | `15.299s` | `12.992s` | MG still fails cold competitiveness |
| `heavy-warm-install` | `10.608s` | `4.461s` | `7.574s` | MG improved, still behind Bun/pnpm for command-total |
| `empty-cache-install` | `15.435s` | `7.229s` | `4.752s` | MG still fails cold competitiveness |
| `warm-install` | `0.933s` | `1.646s` | `5.594s` | MG wins warm/shared local path |

Focused shared-cache new-project measurement:

- Setup:
  - project A and project B use the same `react-vite-basic` dependency surface
  - project A installs first and seeds `MEGAGATE_SHARED_CACHE_DIR`
  - project B is a different directory with no local project cache
- Project B result:

| Metric | Value |
| --- | ---: |
| command wall time | `0.08s` |
| `resolve_graph` | `0ms` |
| `adapter_install` | `81ms` |
| `prefetch_tarballs` | `12ms` |
| packages | `68` |
| bytes from cache | `14791428` |

Read:

- The new shared resolution cache works for the intended “install once, reuse in another project” path.
- This is a real improvement, not a wrapper around npm/pnpm/bun.
- Warm/new-project path is now strong for small and medium React-style projects.
- Cold empty-cache path is still not acceptable for the original “compete with Bun/pnpm” target.
- Heavy cold remains the largest product blocker:
  - MG is `3.66x` slower than pnpm on `heavy-empty-cache-install`.
  - MG is `3.11x` slower than Bun on `heavy-empty-cache-install`.
- The next bottleneck is not local cache reuse anymore; it is first-time metadata/tarball fetch, resolver network shape, and extraction/materialization cost under large graphs.

Next required fixes:

1. Add a real metadata bootstrap/index for popular framework graphs.
2. Add lockfile-first install for generated/scaffolded projects so `mg create` can ship a known-good graph.
3. Stream tarball download into cache/extraction without keeping the current download/cache/extract stages so separated.
4. Add request coalescing across the whole resolver run, not just per batch.
5. Add a dedicated benchmark lane for shared-cache new-project installs so this win stays tracked.

## Update 6: metadata coalescing pass

Additional changes:

- Added an in-process metadata lock map in the web dependency provider.
- `NpmDependencyProvider::metadata` now double-checks cache after acquiring a per-package async lock.
- Batch metadata paths now call the shared `metadata(...)` path instead of manually spawning independent registry fetches.
- This keeps:
  - memory metadata cache
  - shared disk metadata cache
  - stale/fallback policy
  - per-package in-flight lock
  on the same path.

Verified:

- `cargo test -p mg-web-adapter`
- `cargo check -p mg`
- `cargo build --release -p mg`

Benchmark report after coalescing:

- `benchmark_brutal_results_20260718_180739.md`
- `benchmark_brutal_results_20260718_180739.json`
- `benchmark_brutal_results_20260718_180739.status.tsv`

Selected lanes:

| Lane | MG | Bun | pnpm | Current read |
| --- | ---: | ---: | ---: | --- |
| `empty-cache-install` | `15.648s` | `5.316s` | `4.594s` | MG still fails cold competitiveness |
| `heavy-empty-cache-install` | `47.937s` | `16.878s` | `13.592s` | MG still fails heavy cold competitiveness |

Read:

- Metadata coalescing is correct and improves code shape, but it did not materially improve cold benchmark time in this environment.
- This confirms the current critical bottleneck is not only duplicate package metadata requests.
- The cold path still needs a larger design move:
  - lockfile/scaffold graph seed
  - metadata bootstrap index
  - streaming fetch/extract
  - less filesystem churn during first materialization

Current product verdict:

- Warm/shared-cache path: promising.
- Cold empty-cache path: not product-ready.
- Heavy cold path: still a blocker for competing with Bun/pnpm.

## Update 7: cache pressure and GC correctness

Problem found:

- The default global cache on this machine was already about `1.2G`:
  - `/Users/doanmihh/Library/Caches/megagate`
- The previous shared-cache policy only pruned by age:
  - default max age: `7 days`
  - prune interval: `6 hours`
- That is not enough for heavy benchmark loops.
  - A benchmark can fill cache long before files become old.
  - Re-running tests can make global cache bigger and noisier.
- A more serious correctness issue existed in strict layout:
  - `node_modules/.megagate/...` used symlinks to shared extracted cache.
  - If shared cache was deleted or pruned, already-installed projects could break.

Fixes:

- Added CLI cache control:
  - `mg cache status --target shared`
  - `mg cache status --target project`
  - `mg cache status --target build`
  - `mg cache clean --target <all|shared|project|build> --yes`
  - `clean` refuses to delete without `--yes`
- Strict layout now materializes package contents via hardlinks/copy fallback instead of symlinking package roots to shared extracted cache.
  - Project installs no longer depend on shared extracted cache staying alive.
  - Cache GC can prune shared package roots without breaking existing `node_modules`.
- Shared cache GC now includes:
  - tarball cache: `cache/`
  - extracted package cache: `packages/`
  - metadata cache: `metadata/`
  - resolution graph cache: `resolutions/`
- Added quota-based pruning:
  - env: `MEGAGATE_WEB_CACHE_MAX_BYTES`
  - default: `2147483648` bytes (`2 GiB`)
  - behavior: oldest-first deletion until cache is back under budget
  - quota check runs every process, not only every TTL-prune interval
- Added `resolutions/` to age-based prune as well.

Tests added/updated:

- `test_prune_shared_cache_to_quota_removes_prunable_entries`
- `test_install_materialization_uses_hardlinks_from_cached_extract_root`
  - now also removes `shared/packages` after install
  - verifies installed package file still exists and is readable
- `test_cache_command_accepts_status_and_clean_targets`

Verified:

- `cargo test -p mg-web-adapter`
- `cargo test -p mg test_cache_command_accepts_status_and_clean_targets`
- `cargo check -p mg`
- `cargo build --release -p mg`
- `target/release/mg cache status --target shared`
- `target/release/mg cache status --target build`
- `target/release/mg cache clean --target shared`
  - expected refusal without `--yes`

Operational cleanup performed:

- Removed old global MegaGate cache:
  - `/Users/doanmihh/Library/Caches/megagate`
  - previous measured size: about `1.2G`
  - post-clean check: path no longer exists
- Removed old Rust build cache:
  - `/Users/doanmihh/Documents/Workspace/MegaGate/target`
  - previous measured size: `8.4G`
  - release rebuild recreated `target` at about `755M`
  - after adapter tests plus `cargo check -p mg`, `target` measured about `1.8G`
  - after CLI test/check/release rebuild, `mg cache status --target build` measured about `3.7GiB`
- Repeated install cache-growth check:
  - fixture: `react-vite-basic`
  - runs: `5`
  - shared cache: isolated under `/private/tmp`
  - project `.megagate`: `32K -> 36K`, then stable
  - project `node_modules`: stable at `65M`
  - shared cache: stable at `89M`
  - project temp entries after each run: `0`

Read:

- Cache cleanup is now much safer.
- Rebuilding/testing no longer has to rely on manual cache deletion to stay bounded.
- Existing projects should not be coupled to global shared extracted cache anymore.
- Repeated install of the same project no longer shows cache stacking in the measured path.
- This fixes a correctness blocker, not a cold-speed blocker.
- Cold install is still slow until metadata/bootstrap and first materialization are redesigned.

## Update 8: cache command safety and package GC precision

Additional bugs found in the follow-up audit:

- `mg cache status --target build` used `current_dir()/target`.
  - Running the command from `cli/` would report or clean `cli/target` instead of the workspace build cache.
- `mg cache clean --target all --yes` implicitly included the Rust build `target/` directory.
  - That is too destructive for a package-manager cache command and can delete build artifacts while testing.
- Quota GC treated any directory containing `package.json` under shared `packages/` as a prunable package root.
  - Extracted packages can contain nested `package.json` files.
  - That could double-count cache size or delete nested directories that are not MegaGate cache roots.

Fixes added:

- Build cache path now resolves from the Cargo workspace root when possible.
  - Running from nested crates still points to the repository `target/`.
- `clean --target all --yes` no longer includes build artifacts.
  - Build cleanup now requires explicit `mg cache clean --target build --yes`.
- Quota GC now only prunes extracted package roots marked by `.megagate-package-root.json`.
  - Plain nested `package.json` directories are ignored by quota pruning.

New tests:

- `clean_all_does_not_include_build_cache`
- `clean_build_includes_build_cache_explicitly`
- `finds_workspace_target_from_nested_crate`
- `test_prune_shared_cache_to_quota_does_not_delete_unmarked_package_json_dirs`

Verified:

- `cargo fmt`
- `cargo test -p mg-web-adapter prune_shared_cache_to_quota`
- `cargo test -p mg cache`
- `cargo check -p mg`
- `cargo build --release -p mg`
- `target/release/mg cache status --target build`
- `../target/release/mg cache status --target build` from `cli/`
- `target/release/mg cache clean --target all --yes`

Status:

- These fixes reduce cache-management risk and false deletion risk.
- They do not solve the remaining product blocker: cold online install is still slower than Bun/pnpm in the latest measured benchmark.

## Update 9: remove fake security behavior from install/audit

Additional fake/security debt found:

- `mg install` contained a hardcoded quarantine check for package names such as `malicious*` and `react-dom-mock`.
  - This was not a real registry-age check, CVE check, or trust policy.
  - It could create false confidence while missing real risky packages.
- `mg audit --core web` printed simulated vulnerabilities for `lodash` and `minimist`.
  - It slept to mimic network work and printed a fake report.

Fixes added:

- Removed the hardcoded install quarantine rule.
- Routed `mg audit --core web` through the native web adapter audit path.
  - The command now reads `mg.lock` and calls the configured npm advisory provider.
  - The command no longer prints simulated vulnerabilities.
- Fixed `AuditReport::clean(packages_audited)` semantics.
  - It now records packages audited and keeps `vulnerability_count = 0`.
  - Before this fix, passing `package_count` into `clean(...)` could look like a vulnerability count.
- Removed the hardcoded `megagate/0.1.0` audit HTTP user-agent.
  - It now uses the adapter crate version.
- Web adapter audit now errors when the advisory API returns a non-success status instead of silently treating the audit as clean.

Verified:

- `rg` scan no longer finds the removed fake package-name quarantine in CLI/runtime code.
- `cargo fmt`
- `cargo test -p mg cache`
- `cargo check -p mg`
- `cargo build --release -p mg`
- `cargo test -p mg-web-adapter`
- `target/release/mg --core web audit`
  - now uses the adapter audit path instead of the old simulated report.
  - on the repository root, it reported `Packages audited: 0` / `Vulnerabilities: 0` because there is no web lockfile to audit there.
- `rg` scan for removed fake quarantine/hardcoded audit user-agent patterns returned no runtime matches.

Status:

- This is much closer to correct product behavior: no fake security claims, no hardcoded demo quarantine, no silent clean result when the provider fails.
- A full production audit policy is still a blocker before claiming “safe like/better than pnpm/yarn/bun”.
  - Needs offline advisory DB/cache policy.
  - Needs lockfile tamper checks in the audit command.
  - Needs lifecycle script risk reporting.
  - Needs package age/provenance checks based on real registry metadata, not package-name heuristics.

## Update 10: lifecycle portability and common install lock hardening

Additional stability issues found:

- Lifecycle script PATH injection used a hardcoded `:` separator.
  - That is Unix-only and breaks Windows-style PATH handling.
  - It also made the code look like a shell wrapper instead of controlled native setup.
- Lifecycle code described `npm_config_node_gyp` as a mock env var.
  - The env exists for npm ecosystem compatibility, but it must not be documented as a fake/mock behavior.
- Lifecycle parsing silently ignored invalid package manifests.
  - A corrupt or tampered `package.json` could skip script evaluation instead of failing clearly.
- Common `mg install` had older lockfile validation than `mg install-web`.
  - It did not reject unsupported lockfile versions.
  - It did not reject empty package names in lockfile packages.

Fixes added:

- Lifecycle PATH is now built with `std::env::split_paths` / `std::env::join_paths`.
  - This follows the host OS path separator.
- Removed the mock wording from lifecycle env setup.
- Lifecycle now errors on invalid package manifests instead of treating them as empty-script packages.
- Common `mg install` now rejects:
  - unsupported `mg.lock` versions
  - empty package names
- Added tests for:
  - lifecycle PATH prepending `node_modules/.bin`
  - lifecycle invalid `package.json` failure
  - common install rejecting unsupported lockfile versions

Status:

- This improves Windows/Linux/macOS stability for lifecycle execution.
- It also reduces the chance that stale/corrupt locks silently drive install behavior.

Verified:

- `cargo test -p mg-web-adapter lifecycle`
- `cargo test -p mg-web-adapter`
- `cargo test -p mg load_locked_graph`
- `cargo check -p mg`

## Update 11: audit-strict no longer silently no-ops

Additional security surface issue found:

- After removing the fake package-name quarantine, global `--audit-strict` could become a silent no-op for install/add/update flows.
- A security flag that does nothing is not acceptable for product behavior.

Fixes added:

- Dispatch now rejects `--audit-strict` on materializing dependency commands until production strict audit policy is wired.
  - rejected: `install`, `install-web`, `add` / `add-web` when they install, `remove` / `remove-web` when they reinstall, `update --install`
  - allowed: `mg audit`, manifest-only `add-web --no-install`
- Added tests that verify strict mode rejects materializing commands and allows audit/manifest-only paths.

Status:

- MegaGate now fails closed for unsupported strict security mode instead of pretending to enforce it.
- Product blocker remains: strict audit policy still needs real implementation before this flag can be enabled for installs.

Verified:

- `cargo test -p mg audit_strict`
- `cargo build --release -p mg`
- `target/release/mg --audit-strict install-web`
  - exits non-zero with explicit strict-policy-not-wired error.
- `target/release/mg --audit-strict audit --core web`
  - allowed and routes through web adapter audit.
- `target/release/mg cache status --target all`
  - shared/project MegaGate cache: missing/0 B
  - Rust build cache: `5.3 GiB` after this verification build/test pass

## Update 12: lockfile tamper handling is fail-closed in core-web paths

Additional lockfile issue found:

- `read_web_lockfile` returned `Option<Lockfile>`.
  - Checksum mismatch, malformed TOML, or signature errors could collapse into `None`.
  - Important paths could then behave like no lockfile existed, or overwrite lock state during install.

Fixes added:

- Added `read_web_lockfile_checked(project_root) -> MgResult<Option<Lockfile>>`.
- Core-web `list`, `update`, `audit`, and lockfile write paths now use the checked reader.
- Checksum mismatch now returns an error instead of warning/returning `None`.
- Malformed lockfile TOML now returns an error instead of being silently ignored.
- Lockfile signature verification errors now return an error.
- The old `read_web_lockfile` wrapper remains for compatibility and intentionally degrades to `Option`.

Tests added:

- `test_read_web_lockfile_checked_rejects_checksum_mismatch`
- `test_read_web_lockfile_checked_rejects_malformed_lockfile`

Follow-up fix:

- Common `mg install` lock loading now checks `mg.lock.sha256`, malformed TOML, and optional lockfile signature.
- Shared install execution used by `install-web` now checks `mg.lock.sha256`, malformed TOML, and optional lockfile signature.
- Shared pruned-lock reinstall path uses the same checked lock reader.

More tests added:

- common install checksum mismatch rejection
- shared install checksum mismatch rejection

Status:

- Core-web is safer against lockfile tampering and corrupted lockfiles.
- Remaining work: these checked readers should eventually be consolidated into a shared lockfile utility to avoid future drift.

## Update 13: checked lockfile logic consolidated into `mg-lockfile`

Follow-up stability cleanup:

- The checked lockfile logic was duplicated in:
  - web adapter
  - common `mg install`
  - shared install execution
- That duplication could drift again and reintroduce silent checksum/signature handling differences.

Fixes added:

- Added shared helpers to `mg-lockfile`:
  - `lockfile_path`
  - `lockfile_checksum_path`
  - `lockfile_checksum`
  - `write_lockfile_checksum`
  - `read_lockfile_checked`
- Web adapter now writes checksums through `mg-lockfile`.
- Web adapter checked reader now wraps `mg-lockfile::read_lockfile_checked`.
- Common `mg install` and shared install execution now call the same `mg-lockfile` checked reader.
- Removed now-unused `mg-crypto` dependency from:
  - `adapters/web`
  - `cli`

Tests added:

- `mg-lockfile` checksum known-value test
- `mg-lockfile` checked-reader checksum mismatch test

Status:

- Lockfile checksum/signature behavior now has one canonical implementation.
- This reduces code drift and makes future hardening easier.

Verified:

- `cargo test -p mg-lockfile`
- `cargo test -p mg-web-adapter read_web_lockfile_checked`
- `cargo test -p mg checksum_mismatch`
- `cargo check -p mg`
- `cargo test -p mg-web-adapter`
- `cargo build --release -p mg`
- `rg` scan found no remaining `mg-crypto` dependency/use in `cli` or `adapters/web`.

## Update 14: strict lock dependency parsing and atomic checksum writes

Additional stability issues found:

- `graph_from_lockfile` in common install and shared install used `filter_map` for dependency IDs.
  - Malformed dependency IDs inside `mg.lock` were silently dropped.
  - That could materialize an incomplete graph without reporting lock corruption.
- `mg-lockfile::write_lockfile_checksum` used a direct write.
  - A crash/interruption could leave a partial checksum file.

Fixes added:

- Common install lock graph conversion now rejects malformed dependency IDs.
- Shared install lock graph conversion now rejects malformed dependency IDs.
- `mg-lockfile::write_lockfile_checksum` now writes through a temp file and renames into place.

Tests added:

- common install rejects malformed lock dependency ID
- shared install rejects malformed lock dependency ID

Status:

- Lockfile graph loading is now stricter and less likely to silently produce incomplete installs.
- Checksum file writes are more resilient during repeated benchmark/build loops.

Verified:

- `cargo test -p mg invalid_dependency_id`
- `cargo test -p mg-lockfile`
- `cargo check -p mg`
- `cargo build --release -p mg`
- `cargo test -p mg-web-adapter`

## Update 15: lockfile signing key handling is fail-closed

Additional signing issue found:

- `LockfileSigner` treated invalid `MEGAGATE_LOCKFILE_KEY` as if no key existed.
  - Signing could silently no-op when the key was malformed.
- A signed lockfile could be treated as merely unsigned when `MEGAGATE_LOCKFILE_KEY` was missing.
  - That weakens the meaning of `sig`.

Fixes added:

- Invalid `MEGAGATE_LOCKFILE_KEY` now returns an error.
- Signed lockfiles now require `MEGAGATE_LOCKFILE_KEY` during verification.
- Unsigned lockfiles still verify as unsigned when no key is configured.

Tests added:

- invalid lockfile key is rejected
- signed lock without key is rejected
- valid key can sign and verify

Status:

- Lockfile signing now fails closed instead of silently downgrading security.

Verified:

- `cargo test -p mg-lockfile`
- `cargo test -p mg-web-adapter read_web_lockfile_checked`
- `cargo test -p mg checksum_mismatch`
- `cargo check -p mg`
- `cargo build --release -p mg`
- `cargo test -p mg-web-adapter`

## Update 16: stricter CLI surface, monorepo lock verification, and native template commands

Additional issues found after the follow-up scan:

- Monorepo root lock aggregation used the legacy `read_web_lockfile()` helper.
  - That helper returns `Option` and can hide parse/checksum/signature errors.
  - A tampered child workspace lock could be skipped instead of failing the monorepo lock write.
- Web adapter lock writing still downgraded signing failures to a warning.
  - With `MEGAGATE_LOCKFILE_KEY` set incorrectly, the adapter could still write an unsigned lockfile.
- Lifecycle PATH construction used `unwrap_or_default()`.
  - If PATH joining failed, lifecycle scripts could run with an empty PATH instead of returning a clear error.
- `mg create-web --pm ...` still existed as a hidden/no-op compatibility flag.
  - That conflicts with the rule that core-web is native MegaGate, not npm/pnpm/yarn/bun-backed.
- `--ts --js` could be passed together without a validation error.
- Non-web audit modules returned success even though they were not implemented.
  - That is a fake-clean result.
- Web templates still emitted npm commands in CI, Playwright, Vercel/Railway, and Docker scaffolds.
  - This made generated projects look like MegaGate was wrapping npm.

Fixes added:

- Monorepo root lock aggregation now uses `read_web_lockfile_checked()`.
- A child workspace `mg.lock` checksum/signature failure now fails root aggregation.
- Monorepo root `mg.lock` is signed before serialization when `MEGAGATE_LOCKFILE_KEY` is configured.
- Web adapter lock signing now fails closed.
- Lifecycle PATH construction now returns an explicit error on join failure.
- `--pm` is hidden and rejected if supplied.
- `--ts` and `--js` are now mutually exclusive.
- Non-web audit modules now fail clearly instead of returning success.
- Template command surface was rewritten from npm commands to MegaGate commands:
  - `mg install`
  - `mg install --frozen`
  - `mg dev`
  - `mg build`
  - `mg run lint`
  - `mg start`

Tests added:

- monorepo root lock rejects tampered child workspace lock
- external package manager flag is rejected
- `--ts --js` is rejected

Verified:

- `cargo test -p mg write_monorepo_root_lockfile`
- `cargo test -p mg validate_flags`
- `cargo test -p mg-web-adapter lifecycle`
- `cargo test -p mg-web-adapter`
- `cargo test -p mg-lockfile`
- `cargo check -p mg`
- `cargo build --release -p mg`
- `target/release/mg create-web react /private/tmp/mg-pm-reject-check --pm pnpm`
  - expected failure: external package manager flag is rejected before scaffold/network work
- `target/release/mg --core game audit`
  - expected failure: non-web core is not reported as a fake clean audit
- `rg` scan confirms no remaining template commands matching `npm install`, `npm ci`, `npm run dev`, `npm run build`, `npm run lint`, or `CMD ["npm", ...]`.

Important remaining blocker:

- Docker/CI templates now call `mg`, but production packaging must still provide a real MegaGate binary on those runners/images.
- Do not claim Docker/CI production readiness until there is a tested installation path for:
  - macOS
  - Linux
  - Windows
  - CI runners
  - container images

## Update 17: release/install path wired for generated CI and Docker templates

Additional product blocker found:

- Generated CI/Docker templates called `mg`, but clean runners/images did not install the `mg` binary first.
- The GitHub release workflow used one artifact name for every package/target matrix entry.
  - With `actions/upload-artifact@v4`, duplicate artifact names across matrix jobs can fail.
- Release assets did not emit checksum files.
- `scripts/install-from-gh.sh` was hardcoded to `megagate-web`.
  - That blocked reuse for full `megagate` and later single-core packages.

Fixes added:

- `scripts/install-from-gh.sh` now supports:
  - `--package megagate-web`
  - `--package megagate`
  - future `megagate-<core>` package names
- Release workflow now uploads unique artifact names per package/target.
- Release workflow now publishes `.tar.gz.sha256` checksum files.
- Release workflow download step now uses `pattern: release-assets-*` with `merge-multiple: true`.
- Generated GitHub Actions templates now install `megagate-web` before running `mg`.
- Generated Node/Docker web templates now install `megagate-web` before `mg install --frozen`.
- Shared Docker runner stage now copies the `mg` binary before `CMD ["mg", "start"]`.

Verified:

- `bash -n scripts/install-from-gh.sh scripts/install.sh scripts/release.sh scripts/build.sh`
- `cargo check -p mg-dist`
- `rg` scan confirms old npm install/run command surface is still absent from web templates.

Remaining caution:

- Docker/CI install now has a real release path, but it still depends on GitHub Release assets existing.
- Before calling this product-ready, create a tagged release and run generated CI/Docker templates against that release.

## Update 18: single-core web distribution surface verified

Additional release issues found:

- `scripts/install-from-gh.sh` only looked for `mg`, but Windows release archives contain `mg.exe`.
- `install-from-gh.sh` used `install` unconditionally, which is not guaranteed in every Git Bash/Windows-like shell.
- Some package manifests still pointed to the older local-build install script.
- The locally built `megagate-web` binary still showed commands for other cores in `mg --help`.
  - That violates the single-core install model from `DESIGN_FLOW.md`.

Fixes added:

- `install-from-gh.sh` now detects `.exe` on Windows.
- `install-from-gh.sh` now falls back to `cp` when `install` is unavailable.
- All package install hints now point to the GitHub release installer path.
- Per-core CLI commands are hidden from help when their cargo feature is not included.
  - Full `megagate` build still exposes all enabled cores.
  - Single-core `megagate-web` exposes web commands only.

Verified:

- `cargo check -p mg`
- `cargo check -p mg --no-default-features --features web`
- `cargo test -p mg test_help_surface_matches_build_shape`
- `cargo test -p mg --no-default-features --features web test_help_surface_matches_build_shape`
- `cargo run -p mg-dist -- build megagate-web`
- `dist/megagate-web/aarch64-apple-darwin/mg --version`
  - output: `mg 0.2.0`
- `dist/megagate-web/aarch64-apple-darwin/mg --help`
  - shows only `create-web`, `install-web`, `add-web`, `remove-web`, `list-web`, `update-web`
  - no `create-game`, `add-ai`, or other non-web per-core commands
- `dist/megagate-web/aarch64-apple-darwin/mg create-game unity demo`
  - expected failure: only the `web` core is available in this release
- Local distribution output:
  - `dist/megagate-web/aarch64-apple-darwin/mg`
  - size: `19M`
  - receipt mode: `single-core`
  - receipt primary core: `web`

Remaining blocker:

- This verifies local macOS ARM64 packaging only.
- GitHub release matrix must still run for Linux x64, Linux ARM64, macOS x64, macOS ARM64, and Windows x64 before claiming install readiness across OSes.

## Update 19: installer checksum verification is fail-closed

Additional security issue found:

- `scripts/install-from-gh.sh` downloaded and extracted release archives without verifying checksum.
  - The release workflow now emits `.tar.gz.sha256`, but the installer did not enforce it.
  - That left the install path weaker than the release path.

Fixes added:

- Remote installs now download both:
  - `<package>-<target>.tar.gz`
  - `<package>-<target>.tar.gz.sha256`
- Installer verifies SHA-256 before extracting.
- Checksum mismatch fails before installing anything.
- `--archive <tar.gz>` was added for local release artifact verification.
  - This allows testing the installer without requiring a GitHub Release.
- Local archives also require `<tar.gz>.sha256`.

Verified:

- `bash -n scripts/install-from-gh.sh`
- Built local release archive:
  - `/private/tmp/mg-release-local/megagate-web-macOS-ARM64.tar.gz`
  - `/private/tmp/mg-release-local/megagate-web-macOS-ARM64.tar.gz.sha256`
- Installed from local archive:
  - `scripts/install-from-gh.sh --archive /private/tmp/mg-release-local/megagate-web-macOS-ARM64.tar.gz --dir /private/tmp/mg-install-local/bin`
- Installed binary smoke:
  - `/private/tmp/mg-install-local/bin/mg --version`
  - output: `mg 0.2.0`
- Installed single-core help surface:
  - shows web commands only
  - no non-web per-core commands
- Bad checksum test:
  - `scripts/install-from-gh.sh --archive /private/tmp/mg-release-local/bad.tar.gz --dir /private/tmp/mg-install-bad/bin`
  - expected failure: checksum mismatch
- Local artifact sizes:
  - archive: `8.0M`
  - binary: `19M`

Remaining blocker:

- This verifies checksum enforcement on local macOS ARM64 artifacts.
- Remote GitHub Release download still needs to be tested after publishing an actual tag/release.

## Update 20: all single-core feature builds now compile without web adapter leakage

Additional release matrix issue found:

- `available_cores()` always returned all 8 cores.
  - Single-core binaries could show/install/select cores that are not actually included.
- `create_adapter()` referenced `mg_web_adapter` even when the web feature was not enabled.
  - `megagate-ai`, `megagate-game`, and other single-core builds could fail to compile.
- Global `mg info` and `mg outdated` referenced the web registry adapter unconditionally.
  - Non-web single-core builds failed to compile because `mg_web_adapter` was not linked.
- Monorepo root lock aggregation used the web adapter checked lockfile helper.
  - That also leaked the web adapter into non-web build compilation.

Fixes added:

- `available_cores()` is now cargo-feature gated.
  - `megagate-web` reports only web.
  - `megagate-ai` reports only ai.
  - full `megagate` reports all enabled cores.
- `create_adapter()` only references `mg_web_adapter` when the `web` feature is enabled.
- `mg info` and `mg outdated` now fail clearly in non-web builds instead of linking the web adapter.
- Monorepo lock aggregation now uses the shared `mg-lockfile` checked reader directly.
- Tests now validate `available_cores()` for all single-core feature shapes.

Verified:

- `cargo check -p mg`
- `cargo check -p mg-dist`
- `cargo check -p mg --no-default-features --features web`
- `cargo check -p mg --no-default-features --features ai`
- `cargo check -p mg --no-default-features --features game`
- `cargo check -p mg --no-default-features --features clo`
- `cargo check -p mg --no-default-features --features cicd`
- `cargo check -p mg --no-default-features --features iot`
- `cargo check -p mg --no-default-features --features app`
- `cargo check -p mg --no-default-features --features lib`
- `cargo test -p mg --no-default-features --features ai test_available_cores_matches_build_shape`
- `bash -n scripts/install-from-gh.sh scripts/install.sh scripts/build.sh scripts/release.sh`

Remaining blocker:

- Non-web single-core binaries now compile, but their core commands still mostly fail as under-development.
- That is acceptable for release packaging tests, but not for product readiness of those cores.

## Update 21: cache-clean smoke benchmark rerun

Benchmark request:

- Clean cache/build artifacts.
- Rebuild and rerun benchmark.
- Report whether core-web is competitive after the cleanup.

Cleanup performed:

- `target/release/mg cache clean --target build --yes`
  - removed `/Users/doanmihh/Documents/Workspace/MegaGate/target`
- removed temporary local release/install artifacts under `/private/tmp` from previous packaging checks
- Before benchmark:
  - shared cache: `0 B`
  - project cache: `0 B`
  - build cache: removed

Sandbox run:

- `BENCH_MODE=smoke BENCH_RUNS=1 BENCH_WARMUP=0 CONTINUE_ON_FAILURE=1 bash benchmark.sh`
- Result: invalid for performance conclusions.
- Reason:
  - npm registry network blocked
  - Bun tempdir permission denied
  - Go build cache permission denied
- Sandbox report:
  - `benchmark_brutal_results_20260718_232956.md`
  - only `mg-create-web` and `mg-create-web-rich` passed

Real run outside sandbox:

- `BENCH_MODE=smoke BENCH_RUNS=1 BENCH_WARMUP=0 CONTINUE_ON_FAILURE=1 bash benchmark.sh`
- Report:
  - `benchmark_brutal_results_20260718_233323.md`
  - `benchmark_brutal_results_20260718_233323.json`
  - `benchmark_brutal_results_20260718_233323.status.tsv`
- All selected smoke lanes passed:
  - cold-install
  - empty-cache-install
  - add-single
  - build
  - dev-startup
  - monorepo-install
  - mg-create-web
  - mg-create-web-rich
  - heavy-cold-install
  - backend-go-echo

Results:

| Lane | MG | Winner | MG vs Winner |
|---|---:|---:|---:|
| cold-install | 19.428s | pnpm 2.321s | 8.37x slower |
| empty-cache-install | 16.386s | pnpm 4.104s | 3.99x slower |
| add-single | 4.609s | bun 0.688s | 6.70x slower |
| build | 3.672s | bun 1.379s | 2.66x slower |
| dev-startup | 4.064s | bun 1.321s | 3.08x slower |
| monorepo-install | 0.441s | bun 0.108s | 4.10x slower |
| heavy-cold-install | 39.520s | bun 3.481s | 11.35x slower |
| backend-go-echo | 3.147s | MG-only | n/a |
| mg-create-web | 3.530s | MG-only | n/a |
| mg-create-web-rich | 2.220s | MG-only | n/a |

Post-benchmark cache state:

- shared cache: `553.3 MiB`
- project cache: `0 B`
- build cache: `745.7 MiB`
- `target`: `754M`

Conclusion:

- Correctness improved: benchmark no longer fails when run in a real environment.
- Cache cleanup did not solve the core speed problem.
- MG is still not competitive with Bun/pnpm on install/materialization.
- The largest regression is heavy install:
  - MG: `39.520s`
  - Bun: `3.481s`
  - pnpm: `5.420s`
- The profile strongly points to install/materialization/system-time overhead, not scaffold correctness.

Next fix target:

- optimize materialization path before running another full benchmark
- specifically reduce package linking/copying/hardlink filesystem churn
- measure materialization separately from resolve/fetch so the bottleneck is not hidden inside total install time
