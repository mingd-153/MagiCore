# Changelog

All notable changes to MegaGate are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.3.0-beta.1] — 2026-08-20

### 🎉 Highlights

First public beta release. MegaGate is a universal, polyglot package manager for the AI-Agent era — written in Rust with native support for 9 ecosystems and first-class AI coding agent integration.

### Added

**AI-Agent Era Features**
- **`mg mcp`** — Native built-in Model Context Protocol (MCP) server (zero Python dependency). Exposes `mg_install`, `mg_add`, `mg_audit`, `mg_workspace_info` tools to AI IDEs (Cursor, Windsurf, Claude Code, Devin, Antigravity) via JSON-RPC 2.0 stdio.
- **`mg doctor --fix`** — Smart Semantic Doctor with AI-actionable remediation. Detects missing toolchains, read-only store, low disk space, registry unreachability. Outputs structured `DiagnosticIssue` with `fix_command` for automated repair. Health statuses: `HEALTHY` / `DEGRADED` / `UNHEALTHY`.

**Performance & Correctness (Bun/uv/PNPM parity)**
- **Zero-Buffer Pipelined Streaming Download** (`mg-fetcher`): Network chunks stream directly into async file writes via `bytes_stream()` — eliminates full-payload RAM spike for large packages (e.g. Electron, Playwright, PyTorch).
- **OS-Aware Filesystem Concurrency Semaphore** (`mg-platform`): 4 concurrent writes on macOS APFS (eliminates kernel mutex lock contention), 128 on Linux/Windows.
- **Git Conflict Marker Auto-Resolution** (`mg-lockfile`): Automatically 3-way merges `mg.lock` files when encountering Git conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`).
- **Monorepo Catalogs Protocol** (`mg-workspace`): Centralized dependency version management with `catalog:`, `catalog:default`, `catalog:<name>` syntax in `megagate.workspace.toml` (PNPM 11 / Bun compatible).

**Deployment & Infrastructure**
- Docker Compose multi-service setup (`deploy/docker/`) for `mg-registry-server` with hardened non-root container.
- Nginx TLS ≥ 1.2 reverse proxy config (`deploy/nginx/megagate.conf`) with ACME passthrough and IP-restricted admin endpoints.
- 6-target cross-platform release pipeline via GitHub Actions.

**Documentation**
- Root `README.md` with installation guide, quick start, and MCP setup.
- Per-folder `README.md` for all major directories.
- `CONTRIBUTING.md` with full development workflow.

### Changed

- **Version bumped to `0.3.0`** across all 19 workspace crates, Homebrew formula, and Scoop manifest.

### Fixed

- Download implementation no longer buffers entire tarball response in RAM before writing.
- `mg doctor` now reports structured machine-readable JSON (useful for programmatic agent consumption).

---

## [0.2.0] — 2026-07-28

### Added

- 9 ecosystem adapters: `web`, `ai`, `cloud`, `cicd`, `game`, `iot`, `app`, `lib`, `hardware`.
- `mg-registry-server` — embedded and standalone OCI/npm-compatible registry.
- `mg model` — OCI-based AI model push/pull (`hf://` scheme, `mg model push/pull/list`).
- `mg sbom` — CycloneDX 1.5 Software Bill of Materials generation.
- `mg bench` — Install benchmark with wall-time and phase breakdown.
- `mg trust` — Lifecycle script trust gate (opt-in for pre/post scripts).
- `mg hooks` — User-defined pre/post event hooks (`mg.hooks.toml`).
- `mg network` — Full outbound connection transparency listing.
- `mg telemetry` — Opt-in telemetry reporting (off by default).
- Computation caching for monorepo build invalidation.
- Windows PowerShell installer script.

### Changed

- `adapters/web/src/lib.rs` refactored into 12 sub-modules (manifest, cache, provider, install, etc.).
- CAS Store upgraded to v3 format with improved pruning and quota management.

---

## [0.1.0] — 2026-07-16

### Added

- Initial MegaGate CLI (`mg`) with multi-core support.
- Web core: full Node.js/NPM adapter with lockfile, CAS store, lifecycle scripts.
- Project scaffolding: React, Vue, Next.js, Express, Fastify, NestJS, FastAPI, Django, Spring Boot, Gin, Axum, Laravel, Symfony, etc.
- `mg.lock` with SRI integrity verification and BLAKE3 content hashing.
- Content-addressable store (`~/.megagate/store/v3`) with OS reflinks/hardlinks.
- Interactive `mg init` project wizard.
- Global flags: `--core`, `--audit-strict`, `-r/--recursive`.
- Monorepo workspace support with workspace manifests.
- `mg config`, `mg cache`, `mg store`, `mg publish`, `mg patch` commands.

### Known Limitations

- Non-web cores were stubs in v0.1.0 (fully implemented in v0.2.0+).
- HMR is log-only (no WebSocket push).

---

[0.3.0-beta.1]: https://github.com/mingd-153/MegaGate/compare/v0.2.0...v0.3.0-beta.1
[0.2.0]: https://github.com/mingd-153/MegaGate/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/mingd-153/MegaGate/releases/tag/v0.1.0