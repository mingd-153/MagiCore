# Changelog

All notable changes to MagiCore are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Changed
- Test infrastructure improvements (E2E lifecycle tests honest verification)
- CI workflow now triggers on feature/** branches
- Cache tests fixed (race condition removed)
- Install smoke tests added (structure verification)

### Fixed
- Web/AI lifecycle tests now call REAL mgc create commands
- Cache tests use per-command env (no global set_var)
- Removed `|| true` from CI (no silent failures)

### Dependencies
- Updated lightningcss to 1.0.0-alpha.72

---

## [1.1.0-rc.1] - 2026-09-03 (Pending Release)

### 🎯 RC Release - Multi-Core Support

**Status**: Release Candidate for multi-language project management (Web, AI, App, Lib cores).

### Added
- ✅ **Multi-Core Architecture**: Unified CLI for Web, AI, App, Lib ecosystems
- ✅ **Embedded Templates**: Vanilla web, Python AI agent, Flutter app, Rust lib
- ✅ **Lifecycle Commands**: create, install, build, test, run, clean per core
- ✅ **Core Detection**: Auto-detect project type via .mgc.core marker
- ✅ **Adapter System**: Pluggable runtime adapters (npm, pip, cargo, flutter)
- ✅ **Cache System**: Multi-core cache with integrity checks
- ✅ **Security Gates**: Allowlist validation, test runner sandboxing

### Testing
- ✅ Web lifecycle E2E (vanilla embedded template)
- ✅ AI lifecycle E2E (python-agent embedded)
- ✅ Lib lifecycle E2E (Rust/Cargo native)
- ✅ App lifecycle E2E (Flutter - requires SDK)
- ✅ Cache stress tests (5/5 scenarios)
- ✅ Security tests (9/9 allowlist checks)
- ✅ CLI surface tests (7/7 error cases)
- ✅ Install smoke tests (4/4 structure checks)

### Quality Gates
- Clippy: PASS (all targets, -D warnings)
- Format: PASS (rustfmt --check)
- Build: PASS (locked dependencies)
- Tests: 2/4 local (Web ✅, Lib ✅, AI/App require CI runtimes)

### Known Limitations
- Template system uses embedded for core templates (registry-first architecture)
- Some transitive dependencies have security advisories (documented in SECURITY_EXCEPTIONS.toml)
- Install via brew/scoop requires published distributions

---

## [1.0.0] - 2026-08-27

### 🚧 Beta Launch - V1.0.0 (Web)

**Status**: Beta-ready for web projects (npm/pnpm replacement). Multi-language cores experimental.

### Added
- ✅ **SBOM Generation**: `mgc sbom` with CycloneDX/SPDX formats
- ✅ **Signed Lockfiles**: Ed25519 cryptographic signatures (tamper detection)
- ✅ **Trust Policy System**: `mgc trust approve/deny/prune` for lifecycle script control
- ✅ **24-hour Quarantine**: New release safety gate (configurable)
- ✅ **Next.js/React/TypeScript support**: Tested with 20-package dev manifests
- ✅ **Cross-PM Migration**: Import npm/pnpm/yarn/bun lockfiles

### Performance (Beta — Limited Scope)
- **Cold install**: 2.63s average on test workload (macOS M2, 20 packages)
- **Warm install**: 2.01s (pnpm 1.2x faster due to hardlinks)
- **Disk usage**: 462MB (CAS deduplication)
- **Caveat**: Single platform, dev workload only. Cross-platform validation deferred to V1.1

### Known Issues
- ⚠️ **vitest crash**: Projects with `vitest@^1.0.0` encounter "illegal hardware instruction" (workaround: use jest, fix in V1.1)
- ⚠️ **Warm cache**: pnpm slight edge (1.2x) due to hardlink efficiency
- ⚠️ **Multi-language cores**: ai/app/lib remain experimental, reaching parity in V1.1

### Documentation
- Added beta caveats to README and benchmark docs
- Downgraded absolute performance claims to scoped measurements
- RULE §7 compliance: bilingual comments in test code
- Added `V1.0_CRITICAL_BUG_FOUND.md` (vitest crash root cause analysis)

---

## [Unreleased]

### Added
- **Cross-PM lockfile import** (`mgc import`): migrate `package-lock.json` / `pnpm-lock.yaml` / `yarn.lock` / `bun.lock` → signed `mgc.lock` v2. Shape-first version policy — new PM format bumps import without an mgc release (advisory warning only). Install auto-seeds from legacy lockfiles when the manifest matches.
- **Lockfile 3-way merge** (`merge3`) restored on schema v2, including automatic git conflict-marker resolution (add/add same-version keeps, divergent versions fail closed).
- **Canonical TOML lockfile writer** — all writers now emit one format; readers accept both new TOML and legacy JSON-flavoured files.
- **Black-box E2E crate** `tests/e2e`: real-binary pipelines pack→publish→install and import→install against a local registry (fully hermetic).
- **Hermetic native-client tests**: cargo sparse-index + PyPI JSON API covered via mockito (no network).

### Planned (carried over)
- Offline mode R1: pass `ResolveOptions { offline }` through `PackageAdapter::resolve`; R4: enforce no-network in offline mode (moved from `cli/src/commands/install.rs.TODO`).
---

## [1.0.0] — 2026-08-21 🚧 BETA RELEASE (WEB)

### 🚧 Status

**Beta-ready for web projects.** MagiCore V1.0.0 includes SBOM generation, cryptographically signed lockfiles, and trust policies. Web (npm/pnpm replacement) is beta-ready; multi-language cores (ai/app/lib) remain experimental. Full core parity targeted for V1.1.

### Added

**Week 6: SBOM Generation (Supply Chain Security)**
- **`mgc sbom`** — Generate Software Bill of Materials in CycloneDX JSON/XML and SPDX JSON formats
  - Full dependency tree analysis with transitive dependencies
  - License detection and compliance checking
  - Vulnerability mapping (CVE references)
  - Supports all adapters: web, game, ai, clo, cicd, iot
  - Output formats: `--format cyclonedx-json|cyclonedx-xml|spdx-json`
  - Configurable output: `--output <file>` or stdout
  - Workspace support: `--dir <path>` for multi-project analysis

**Lockfile v2 (Cryptographic Signing)**
- Ed25519 signature-based lockfile verification (tamper detection)
- Lockfile schema v2 with simplified Package structure
- Automatic v1 → v2 migration on first install
- Signature files (`.mgc.lock.sig`) stored alongside lockfiles
- Public key distribution via trusted keyring

**CLI Enhancements**
- `--offline` flag for all install commands (network-free mode)
- `--dir` flag for SBOM generation (workspace scanning)
- `--name` and `--version` flags for custom SBOM metadata
- Improved error messages with actionable suggestions

### Changed

- **BREAKING**: Lockfile v1 schema deprecated (auto-migrated to v2)
  - Removed fields: `lock.resolution`, `lock.core`, `lock.mode`, `lock.frameworks`
  - Removed package fields: `pkg.direct`, `pkg.dev`
  - Simplified structure: `Package` (was `LockPackage`)
  
- **Version bumped to `1.0.0`** — API stability guarantee
- All 19 workspace crates updated to 1.0.0
- Cargo.toml workspace version: `0.4.0` → `1.0.0`

### Fixed

- H2 vulnerability (RUSTSEC-2026-0258) patched → v0.4.19
- Duplicate Sbom command definitions resolved
- InstallWeb missing offline field added
- 96 compilation errors from lockfile migration resolved

### Known Limitations (V1.0.1 Hotfix Plan)

⚠️ **Temporarily Disabled Features** (stubbed for rapid V1.0.0 release):
- Workspace lockfile merging (`write_monorepo_root_lockfile`)
- Pruned install optimization (`load_pruned_locked_graph`)
- Dependency explanation (`mgc why` command)
- Lockfile version checks (compatibility validation)
- ~80% of test suite (requires v2 schema rewrite)

These features will be restored in **V1.0.1 hotfix (Week 7)** — estimated 1 week.

### Security

**Audit Status**: ✅ Approved for release (see `SECURITY_AUDIT_V1.0.0.md`)

**Fixed**:
- ✅ H2 unbounded DATA frames (RUSTSEC-2026-0258)

**Documented** (V1.0.1 fixes):
- ⚠️ quick-xml v0.37.5 (2 CVEs, severity 7.5) — transitive via object_store
- ⚠️ rkyv v0.7.46 (out-of-bounds reads) — transitive via lightningcss
- ⚠️ rsa v0.9.10 (Marvin Attack) — no upstream fix available
- ⚠️ 7 unmaintained crates (bincode, paste, rustls-pemfile, etc.)

**Mitigation**: Registry server disabled by default. Core CLI (install, SBOM, lockfile) unaffected.

**Recommendation**: 
- ✅ Safe for CLI usage (install, add, remove, SBOM)
- ⚠️ Wait for V1.0.1 before deploying registry server to production

See full security report: `SECURITY_AUDIT_V1.0.0.md`

### Migration Guide

**Lockfile v1 → v2 (Automatic)**:
```bash
# Backup existing lockfile (optional)
cp mgc.lock mgc.lock.v1.backup

# Run any install command — auto-migrates to v2
mgc install web

# Verify signature
ls -la mgc.lock.sig  # signature file created
```

**SBOM Generation**:
```bash
# Generate CycloneDX JSON
mgc sbom --format cyclonedx-json --output sbom.json

# Generate SPDX JSON
mgc sbom --format spdx-json --output sbom.spdx.json

# Custom metadata
mgc sbom \
  --format cyclonedx-json \
  --name "MyApp" \
  --version "1.0.0" \
  --output sbom.json
```

### Deprecations

- Lockfile v1 schema (auto-migrated, no action required)
- `LockPackage` type (renamed to `Package`)
- Legacy lockfile functions (replaced with v2 API)

### Contributors

Special thanks to the MagiCore community for testing, bug reports, and feedback throughout the beta phase!

---

## [0.3.0-beta.1] — 2026-08-20

### 🎉 Highlights

First public beta release. MagiCore is a universal, polyglot package manager for the AI-Agent era — written in Rust with native support for 9 ecosystems and first-class AI coding agent integration.

### Added

**AI-Agent Era Features**
- **`mgc mcp`** — Native built-in Model Context Protocol (MCP) server (zero Python dependency). Exposes `mgc_install`, `mgc_add`, `mgc_audit`, `mgc_workspace_info` tools to AI IDEs (Cursor, Windsurf, Claude Code, Devin, Antigravity) via JSON-RPC 2.0 stdio.
- **`mgc doctor --fix`** — Smart Semantic Doctor with AI-actionable remediation. Detects missing toolchains, read-only store, low disk space, registry unreachability. Outputs structured `DiagnosticIssue` with `fix_command` for automated repair. Health statuses: `HEALTHY` / `DEGRADED` / `UNHEALTHY`.

**Performance & Correctness (Bun/uv/PNPM parity)**
- **Zero-Buffer Pipelined Streaming Download** (`mgc-fetcher`): Network chunks stream directly into async file writes via `bytes_stream()` — eliminates full-payload RAM spike for large packages (e.g. Electron, Playwright, PyTorch).
- **OS-Aware Filesystem Concurrency Semaphore** (`mgc-platform`): 4 concurrent writes on macOS APFS (eliminates kernel mutex lock contention), 128 on Linux/Windows.
- **Git Conflict Marker Auto-Resolution** (`mgc-lockfile`): Automatically 3-way merges `mgc.lock` files when encountering Git conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`).
- **Monorepo Catalogs Protocol** (`mgc-workspace`): Centralized dependency version management with `catalog:`, `catalog:default`, `catalog:<name>` syntax in `magicore.workspace.toml` (PNPM 11 / Bun compatible).

**Deployment & Infrastructure**
- Docker Compose multi-service setup (`deploy/docker/`) for `mgc-registry-server` with hardened non-root container.
- Nginx TLS ≥ 1.2 reverse proxy config (`deploy/nginx/magicore.conf`) with ACME passthrough and IP-restricted admin endpoints.
- 6-target cross-platform release pipeline via GitHub Actions.

**Documentation**
- Root `README.md` with installation guide, quick start, and MCP setup.
- Per-folder `README.md` for all major directories.
- `CONTRIBUTING.md` with full development workflow.

### Changed

- **Version bumped to `0.3.0`** across all 19 workspace crates, Homebrew formula, and Scoop manifest.

### Fixed

- Download implementation no longer buffers entire tarball response in RAM before writing.
- `mgc doctor` now reports structured machine-readable JSON (useful for programmatic agent consumption).

---

## [0.2.0] — 2026-07-28

### Added

- 9 ecosystem adapters: `web`, `ai`, `cloud`, `cicd`, `game`, `iot`, `app`, `lib`, `hardware`.
- `mgc-registry-server` — embedded and standalone OCI/npm-compatible registry.
- `mgc model` — OCI-based AI model push/pull (`hf://` scheme, `mgc model push/pull/list`).
- `mgc sbom` — CycloneDX 1.5 Software Bill of Materials generation.
- `mgc bench` — Install benchmark with wall-time and phase breakdown.
- `mgc trust` — Lifecycle script trust gate (opt-in for pre/post scripts).
- `mgc hooks` — User-defined pre/post event hooks (`mgc.hooks.toml`).
- `mgc network` — Full outbound connection transparency listing.
- `mgc telemetry` — Opt-in telemetry reporting (off by default).
- Computation caching for monorepo build invalidation.
- Windows PowerShell installer script.

### Changed

- `adapters/web/src/lib.rs` refactored into 12 sub-modules (manifest, cache, provider, install, etc.).
- CAS Store upgraded to v3 format with improved pruning and quota management.

---

## [0.1.0] — 2026-07-16

### Added

- Initial MagiCore CLI (`mgc`) with multi-core support.
- Web core: full Node.js/NPM adapter with lockfile, CAS store, lifecycle scripts.
- Project scaffolding: React, Vue, Next.js, Express, Fastify, NestJS, FastAPI, Django, Spring Boot, Gin, Axum, Laravel, Symfony, etc.
- `mgc.lock` with SRI integrity verification and BLAKE3 content hashing.
- Content-addressable store (`~/.magicore/store/v3`) with OS reflinks/hardlinks.
- Interactive `mgc init` project wizard.
- Global flags: `--core`, `--audit-strict`, `-r/--recursive`.
- Monorepo workspace support with workspace manifests.
- `mgc config`, `mgc cache`, `mgc store`, `mgc publish`, `mgc patch` commands.

### Known Limitations

- Non-web cores were stubs in v0.1.0 (fully implemented in v0.2.0+).
- HMR is log-only (no WebSocket push).

---

[0.3.0-beta.1]: https://github.com/mingd-153/MagiCore/compare/v0.2.0...v0.3.0-beta.1
[0.2.0]: https://github.com/mingd-153/MagiCore/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/mingd-153/MagiCore/releases/tag/v0.1.0
