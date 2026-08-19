# `core/` — MegaGate Core Crates

All foundational Rust library crates. None of these crates contain CLI logic — they are pure libraries used by the CLI and adapters.

## Crate Map

| Crate | Purpose |
|---|---|
| `mg-types` | Shared types, package IDs, error traits, semver wrappers |
| `mg-platform` | OS abstraction: reflink/hardlink, `FsSemaphore` (APFS concurrency tuning) |
| `mg-crypto` | BLAKE3 content hashing, SHA-256/SHA-512, SRI integrity strings |
| `mg-http` | Resilient async HTTP client with retries, rate-limiting, ETags |
| `mg-store` | Content-Addressable Storage (CAS) `~/.megagate/store/v3` — import, claim, prune |
| `mg-fetcher` | Streaming tarball download (zero-buffer) + secure extraction into CAS |
| `mg-resolver` | Dependency graph SAT solver: range resolution, alias handling, deduplication |
| `mg-lockfile` | `mg.lock` read/write, BLAKE3 checksum, 3-way Git conflict auto-resolution |
| `mg-workspace` | Monorepo topology, workspace graph, computation caching, Catalogs protocol |
| `mg-config` | 5-tier ordered config engine (`.npmrc`, `mg.toml`, env vars, CLI flags) |
| `mg-exec` | Sandboxed subprocess execution for lifecycle scripts |
| `mg-registry-server` | Embedded npm/OCI-compatible registry server |
| `mg-oci` | OCI image push/pull for AI model management |
| `mg-pack` | Tarball packaging for `mg publish` |
| `mg-publish` | Publish pipeline: version bump, registry upload, tag |
| `mg-plugin` | Plugin loading API |
| `mg-adapter-base` | Base traits all ecosystem adapters must implement |
| `mg-ui` | Terminal output: spinners, progress bars, colored logs |

## Key Design Principles

- **No circular dependencies.** Dependency order: `mg-types` → `mg-platform`, `mg-crypto` → `mg-store` → `mg-fetcher`, `mg-resolver` → `mg-lockfile` → `mg-workspace`.
- **Fail-closed.** All security-critical operations (extraction, CAS write) bail on any unexpected input.
- **Async-first.** All I/O uses Tokio. CPU-heavy work (decompression, hashing) uses Rayon thread pool.
- **Cross-platform.** macOS (APFS reflinks), Linux (btrfs/ext4 hardlinks), Windows (NTFS hardlinks).
