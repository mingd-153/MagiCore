# Web Adapter (mgc-web-adapter)

NPM/Node.js package manager adapter for MagiCore.

## Commands

| Command | Description |
|---------|-------------|
| `mgc create <framework> <project> [--ts] [--tailwindcss] [--monorepo] [--backend <fw>]` | Single-core web scaffold |
| `mgc add <packages...> [-D] [-E] [-O] [-P] [--no-save] [-g]` | Single-core add |
| `mgc install [packages...]` | Single-core install |
| `mgc create-web <framework> <project> [--ts] [--tailwindcss] [--monorepo] [--backend <fw>]` | Multi-core web scaffold |
| `mgc add-web <packages...> [-D] [-E] [-O] [-P] [--no-save] [-g]` | Multi-core add |
| `mgc remove-web <packages...>` | Multi-core remove |
| `mgc list-web` | Multi-core list |
| `mgc update-web [packages]` | Multi-core update |
| `mgc install-web [packages]` | Multi-core install |

## Test

```bash
# Run web adapter tests
cargo test -p mgc-web-adapter

# Run CLI web commands tests
cargo test -p mgc
```

## Benchmark

```bash
# Criterion benches
./scripts/bench.sh cold
./scripts/bench.sh stress
./scripts/bench.sh install

# Standardized install/materialization matrix
./scripts/bench.sh matrix
./scripts/bench.sh matrix-heavy

# Save/load matrix baseline
./scripts/bench.sh matrix-baseline
./scripts/bench.sh matrix-diff
./scripts/bench.sh matrix-heavy-baseline
./scripts/bench.sh matrix-heavy-diff
```

The matrix runner isolates five install/resolve scenarios so numbers are easier to compare across changes:

- `cold-local-cache`: fresh project, local tarballs already present, no `node_modules`
- `warm-reinstall`: same project installed twice, second run measured
- `cold-online-registry`: fresh project resolved from a local mock registry with empty local/shared cache
- `shared-cache-bootstrap`: fresh project bootstrapped from MagiCore shared cache
- `offline-cached-install`: registry intentionally unreachable, local tarball/extracted cache already exists, install must succeed from cache only

The `heavy` profile adds a larger graph with:

- more direct packages
- deep transitive dependencies
- duplicate package names on different versions
- nested materialization pressure closer to real monorepo/frontend toolchains

This matrix is intentionally narrower than the online registry path. It standardizes install/materialization behavior first; cold online fetch remains a separate performance track.

Metadata fallback policy:

- fresh metadata uses `MAGICORE_WEB_METADATA_TTL_SECS` (default `300`)
- stale retry backoff uses `MAGICORE_WEB_METADATA_STALE_RETRY_TTL_SECS` (default `30`)
- stale metadata fallback is capped by `MAGICORE_WEB_METADATA_MAX_STALE_SECS` (default `604800`)

Shared cache hygiene:

- shared cache discovery now runs a best-effort prune pass
- cache prune cadence uses `MAGICORE_WEB_CACHE_PRUNE_INTERVAL_SECS` (default `21600`)
- stale shared tarballs / extracted package roots / metadata entries age out via `MAGICORE_WEB_CACHE_MAX_AGE_SECS` (default `2592000`)
- retry-deferred metadata is still blocked by the max-stale rule; retry cooldown no longer bypasses stale safety

## Integration

Each command routes through `cli/src/commands/core/web.rs` which creates a `WebAdapter` and delegates to `shared.rs` for common logic (add, remove, list, update, install).

Single-core web builds expose the bare web surface:

- `mgc create <framework> <project> ...` as an alias to the web scaffold entrypoint
- `mgc add`, `mgc remove`, `mgc update`, `mgc list`, `mgc install` through the auto-detected bare command path

Multi-core builds expose per-core web commands (`mgc create-web`, `mgc add-web`, `mgc install-web`).
