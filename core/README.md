# `core/` — MagiCore Core Crates

All foundational Rust library crates. None of these crates contain CLI logic — they are pure libraries used by the CLI and adapters.

## Crate Map

| Crate | Purpose |
|---|---|
| `mgc-types` | Shared types, package IDs, error traits, semver wrappers |
| `mgc-platform` | OS abstraction: reflink/hardlink, `FsSemaphore` (APFS concurrency tuning) |
| `mgc-crypto` | BLAKE3 content hashing, SHA-256/SHA-512, SRI integrity strings |
| `mgc-http` | Resilient async HTTP client with retries, rate-limiting, ETags |
| `mgc-store` | Content-Addressable Storage (CAS) `~/.magicore/store/v3` — import, claim, prune |
| `mgc-fetcher` | Streaming tarball download (zero-buffer) + secure extraction into CAS |
| `mgc-resolver` | Dependency graph SAT solver: range resolution, alias handling, deduplication |
| `mgc-lockfile` | `mgc.lock` read/write, BLAKE3 checksum, 3-way Git conflict auto-resolution |
| `mgc-workspace` | Monorepo topology, workspace graph, computation caching, Catalogs protocol |
| `mgc-config` | 5-tier ordered config engine (`.npmrc`, `mgc.toml`, env vars, CLI flags) |
| `mgc-exec` | Sandboxed subprocess execution for lifecycle scripts |
| `mgc-registry-server` | Embedded npm/OCI-compatible registry server |
| `mgc-oci` | OCI image push/pull for AI model management |
| `mgc-pack` | Tarball packaging for `mgc publish` |
| `mgc-publish` | Publish pipeline: version bump, registry upload, tag |
| `mgc-plugin` | Plugin loading API |
| `mgc-adapter-base` | Base traits all ecosystem adapters must implement |
| `mgc-ui` | Terminal output: spinners, progress bars, colored logs |

## Key Design Principles

- **No circular dependencies.** Dependency order: `mgc-types` → `mgc-platform`, `mgc-crypto` → `mgc-store` → `mgc-fetcher`, `mgc-resolver` → `mgc-lockfile` → `mgc-workspace`.
- **Fail-closed.** All security-critical operations (extraction, CAS write) bail on any unexpected input.
- **Async-first.** All I/O uses Tokio. CPU-heavy work (decompression, hashing) uses Rayon thread pool.
- **Cross-platform.** macOS (APFS reflinks), Linux (btrfs/ext4 hardlinks), Windows (NTFS hardlinks).
