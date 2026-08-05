# Changelog

All notable changes to MegaGate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- LICENSE file (MIT)
- CI/CD GitHub Actions workflow (`.github/workflows/ci.yml`)
- CONTRIBUTING.md with development guidelines
- `mg link`, `mg unlink`, `mg why` commands with real npm/pnpm integration
- `mg i`, `mg rm`, `mg up`, `mg ls` aliases for install/remove/update/list
- `mg.toml` at project root (replaces `.megagate/project.toml`)
- `detect_ecosystem` now uses `mg.toml` and prompts user on ambiguity
- Interactive prompt creates `mg.toml` instead of `.megagate/project.toml`
- Dead-code warnings eliminated (0 warnings on build)

### Changed
- `mg init` creates `mg.toml` at project root with real scaffold data
- `mg dev --port` properly binds to custom port
- `mg create-web` writes full scaffold metadata to `mg.toml`
- CLI doc (`CLI_FULL_SURFACE_2026-07-16.md`) aligned with actual implementation
- All dead-code warnings fixed (`#![allow(dead_code)]` where appropriate)

### Fixed
- Core overlap: projects with both `package.json` and `Cargo.toml` now prompt user instead of auto-detecting web
- `detect_ecosystem` no longer checks deprecated `.megagate/project.toml`
- `find_project_root` now prioritizes `mg.toml` over `package.json`

### Removed
- Unused `available_core_names()` function
- Unused `success` import in audit/web.rs
- Unused variable `other` in build.rs

## [0.1.0] - 2026-07-16

### Added
- Initial MegaGate CLI with multi-core support (web, game, ai, clo, cicd, iot, app, lib)
- Web core with full adapter implementation
- Project scaffolding with templates for React, Vue, Next.js, Express, Fastify, NestJS, etc.
- Lockfile (`mg.lock`) with integrity verification
- Dev server with file watching and hot reload logging
- Package management: install, add, remove, update, list
- Dependency resolution with caching and registry metadata
- Content-addressable store with content deduplication
- Interactive project wizard (`mg init`)
- Global flags: `--core`, `--audit-strict`, `-r/--recursive`
- Namespaced commands: `create-web`, `install-web`, `add-web`, etc.
- Template system with feature flags (TS, Tailwind, ESLint, Vitest, Playwright, Docker, etc.)
- Monorepo support with workspace manifests
- Cross-language backend support (Node, Go, Python, Rust, Java, PHP)

### Known Issues
- `mg build` delegates to `run build` (not native Rust bundler)
- HMR is log-only (no WebSocket push)
- Non-web cores (game, ai, clo, etc.) are stubs
- Only NPM registry supported (no Go/Python/Rust registries)
- `mg link/unlink/why` were stubs (fixed in Unreleased)
- Missing LICENSE, CI/CD, CONTRIBUTING, CHANGELOG (fixed in Unreleased)