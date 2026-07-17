# Phase A Web Report

Date: 2026-07-11

## Scope

Phase A for `mg/web` focused on:

- offline metadata/tarball fallback
- shared cache reuse across projects
- flaky network retry handling
- monorepo root install behavior
- stress testing resolver/install paths
- benchmark baselines with repeatable fixtures

## Code Changes Landed

- `adapters/web/src/lib.rs`
  - added shared disk cache discovery via `MEGAGATE_SHARED_CACHE_DIR` or OS cache dir
  - added metadata fallback to shared cache when registry fetch fails
  - added shared tarball fallback so a fresh project can reuse an already-cached package
  - added `MEGAGATE_WEB_REGISTRY_URL` override for fail-fast/offline testing
- `adapters/web/src/native/npm_registry.rs`
  - added retry loop for transient metadata/tarball failures
- `cli/src/commands/install.rs`
  - removed artificial `sleep` delays from install progress
  - changed displayed install duration to end-to-end command duration instead of only adapter link time
- `core/crates/mg-resolver/src/solver/mod.rs`
  - added larger tree stress test
- `core/crates/mg-resolver/benches/resolver_bench.rs`
  - fixed deep-tree benchmark naming/level parsing
  - added `solver_tree_1555_pkgs`
- `adapters/web/benches/install_bench.rs`
  - added `web_install_cached_25_pkgs_40_files_each`

## Automated Verification

| Suite | Result |
| --- | --- |
| `cargo test -p mg-web-adapter --lib -- --nocapture` | 20 passed |
| `cargo test -p mg-resolver --lib -- --nocapture` | 15 passed |
| `cargo test -p mg commands::install::tests -- --nocapture` | 4 passed |

Notable new coverage:

- offline `add` from shared metadata cache
- install from shared tarball cache into a brand-new project
- transient metadata retry
- transient tarball retry
- larger resolver tree without duplicate explosion

## Synthetic Offline Fixture

Fixture:

- 25 flat packages
- shared metadata cache pre-seeded
- shared tarball cache pre-seeded
- registry redirected to `http://127.0.0.1:9` for fast offline fallback

Disk footprint:

| Path | Size |
| --- | ---: |
| shared fixture cache | 200 KB |
| single-project `node_modules` | 4100 KB |
| single-project local `.megagate/cache/web` | 120 KB |
| monorepo frontend `node_modules` | 1968 KB |
| monorepo backend `node_modules` | 1312 KB |
| monorepo contracts `node_modules` | 820 KB |

Observed outputs after removing fake CLI sleeps:

| Scenario | Packages | Reported total |
| --- | ---: | ---: |
| single-core offline reinstall | 25 | 49 ms |
| monorepo backend reinstall | 8 | 4 ms |
| monorepo frontend reinstall | 12 | 5 ms |
| monorepo contracts reinstall | 5 | 3 ms |

## Real Project Smoke

Template:

- `react-vite`
- `--ts`
- `--tailwindcss`

Observed outputs:

| Scenario | Packages | Reported total | Outer wall time |
| --- | ---: | ---: | ---: |
| web-only warm reinstall | 71 | 33 ms | 0.04 s |
| all-core warm reinstall (`install-web`) | 71 | 40 ms | 0.08 s |

Earlier cold-path observation before removing fake CLI sleeps:

| Scenario | Packages | Reported total | Outer wall time | Max RSS |
| --- | ---: | ---: | ---: | ---: |
| web-only install | 71 | 4940 ms | 66.49 s | 180224000 |
| all-core install-web | 71 | 10630 ms | 66.58 s | 221446144 |

Interpretation:

- warm path is now genuinely fast once `mg.lock` and materialized packages exist
- previous warm slowness was dominated by artificial CLI progress sleeps, now removed
- cold path is still much heavier than desired and remains the next optimization target

## Criterion Benchmarks

### Resolver

| Benchmark | Result |
| --- | --- |
| `matches_caret` | 51.841 ns - 58.555 ns |
| `matches_tilde` | 59.215 ns - 98.611 ns |
| `matches_star` | 2.4556 ns - 5.3932 ns |
| `matches_or` | 93.195 ns - 166.69 ns |
| `solver_single_10k_versions` | 663.95 us - 695.64 us |
| `solver_tree_156_pkgs` | 245.79 us - 380.32 us |
| `solver_tree_1555_pkgs` | 2.1545 ms - 2.2260 ms |

### Cached Install

| Benchmark | Result |
| --- | --- |
| `web_install_cached_single_pkg_50_files` | 13.009 ms - 13.638 ms |
| `web_install_cached_5_pkgs_20_files_each` | 25.367 ms - 25.989 ms |
| `web_install_cached_25_pkgs_40_files_each` | 217.72 ms - 233.86 ms |

## Audit Notes

### Strengthened

- shared cache now works as a real cross-project bootstrap source
- metadata fallback no longer forces stale cache first; online fetch still wins when available
- retry path now handles one-shot transient failures
- monorepo root install now walks workspace manifests instead of assuming a root `package.json`

### Remaining Gaps

- integrity from registry metadata is not yet enforced during tarball reuse/materialization
- package materialization still copies files into `node_modules`; no hardlink/symlink dedupe yet
- cold online path is still too slow for the project ambition and needs deeper resolver/fetch/link profiling
- there is no persistent freshness/TTL policy yet for on-disk metadata beyond “prefer network, fallback to disk”
