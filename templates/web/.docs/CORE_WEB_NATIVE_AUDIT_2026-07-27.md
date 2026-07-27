# Core-Web Native Audit

Date: 2026-07-27

## Scope

This audit checks the current MegaGate web core against the stated product goal:

- not a wrapper around npm / pnpm / bun / yarn
- native core behavior owned by MegaGate
- honest cross-language support expectations
- no fabricated product claims

## Executive verdict

### What is true today

1. **Web dependency management core is native MegaGate**
   - `install`, `add`, `remove`, `update`, `list`, `why`, `build` for the web dependency graph run through MegaGate code, not through npm/pnpm/bun/yarn.
2. **Wrapper escape hatches have been reduced**
   - `mg dev` rejects `package.json` scripts that delegate to `npm`, `pnpm`, `bun`, `yarn`, `npx`, `bunx`
   - `mg run` rejects the same wrapper pattern
   - lifecycle scripts now reject the same wrapper pattern
3. **Cold and warm install performance improved**
   - latest focused benchmark runs show the strict-layout critical path got shorter

### What is not true today

1. **MegaGate is not yet a native multi-language package manager for Go / Java / Python / PHP**
   - today it scaffolds and launches those stacks
   - it does not yet resolve, lock, cache, audit, dedupe, and materialize those ecosystems the same way it does for web/node packages
2. **Security policy is not yet “absolute”**
   - some integrity paths are strong
   - some policy gates are still partial or not wired across the full surface
3. **“Faster than Bun” is still not an honest global claim**
   - some steady paths are excellent
   - cold install leadership is still incomplete

## Findings

### P0 — Cross-language PM claim is ahead of implementation

- **Severity**: Critical
- **Why it matters**: the product vision asks for one intelligent manager across web/fullstack/backend/monorepo and multiple languages
- **Current reality**:
  - Go backend dev/install helpers still rely on `go` toolchain flows in command/runtime paths
  - Rust backend helper paths still rely on `cargo`
  - Python backend helper paths still rely on `python`, `venv`, `pip`
  - Java backend helper paths still rely on `mvn`
  - PHP backend helper paths still rely on `php` / `composer`
- **Evidence**:
  - [cli/src/commands/core/web.rs](/Users/doanmihh/Documents/Workspace/MegaGate/cli/src/commands/core/web.rs)
  - [cli/src/commands/build.rs](/Users/doanmihh/Documents/Workspace/MegaGate/cli/src/commands/build.rs)
  - template Dockerfiles under [templates/web/backend](/Users/doanmihh/Documents/Workspace/MegaGate/templates/web/backend)
- **Conclusion**:
  - MegaGate currently has a native **web/node dependency engine**
  - it does **not** yet have equivalent native engines for Go, Java, Python, PHP

### P0 — Audit-strict / quarantine policy is not fully wired

- **Severity**: Critical
- **Why it matters**: the requested bar includes pnpm-like 24h quarantine or stronger
- **Current reality**:
  - CLI already admits `--audit-strict` is not fully wired for production materialization commands
  - unsupported paths are refused honestly, but the full promised policy is not yet implemented end-to-end
- **Evidence**:
  - [cli/src/dispatch.rs](/Users/doanmihh/Documents/Workspace/MegaGate/cli/src/dispatch.rs)
- **Conclusion**:
  - this is safer than fake success
  - but it is still not the finished product claim

### P1 — Native web core is real, but framework runtime still depends on framework binaries

- **Severity**: High
- **Why it matters**: “no PM wrappers” does not mean “no framework runtime”
- **Current reality**:
  - MegaGate installs and links local binaries, then runs framework-local commands such as `vite`, `next`, `nuxt`, `astro`, `ng`, `tsx`
  - this is acceptable as framework runtime, not package-manager delegation
- **Evidence**:
  - [cli/src/commands/core/web.rs](/Users/doanmihh/Documents/Workspace/MegaGate/cli/src/commands/core/web.rs)
- **Conclusion**:
  - native PM goal is preserved
  - full runtime replacement for every framework is not implemented and should not be falsely claimed

### P1 — PM-wrapper escape hatches were real and are now partially closed

- **Severity**: High
- **Fixes now in place**:
  - `mg dev` rejects wrapper scripts
  - `mg run` rejects wrapper scripts
  - lifecycle execution rejects wrapper scripts
- **Evidence**:
  - [cli/src/commands/core/web.rs](/Users/doanmihh/Documents/Workspace/MegaGate/cli/src/commands/core/web.rs)
  - [cli/src/commands/run.rs](/Users/doanmihh/Documents/Workspace/MegaGate/cli/src/commands/run.rs)
  - [adapters/web/src/lifecycle.rs](/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/lifecycle.rs)
- **Conclusion**:
  - this materially improves architectural honesty
  - more shell-surface hardening is still possible

### P1 — Memory / cache intelligence is meaningful but not yet a full cross-language system

- **Severity**: High
- **Current strengths**:
  - shared cache
  - extracted package reuse
  - lockfile reuse
  - incremental install paths
  - strict-layout materialization
  - no-op manifest / lockfile avoidance
- **Evidence**:
  - [adapters/web/src/lib.rs](/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/lib.rs)
- **Limit**:
  - these optimizations are still centered on the web/node ecosystem
  - no equivalent unified cache/materializer exists yet for Go module cache, Maven artifacts, Python wheels, Composer vendor trees

### P2 — Security foundation exists, but not “absolute security”

- **Severity**: Medium
- **Current strengths**:
  - SRI verification paths
  - weak hash rejection in strict mode
  - lockfile checksum support
  - shared cache corruption recovery tests
- **Evidence**:
  - [adapters/web/src/lib.rs](/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/lib.rs)
  - [core/crates/mg-store](/Users/doanmihh/Documents/Workspace/MegaGate/core/crates/mg-store)
  - [core/crates/mg-crypto](/Users/doanmihh/Documents/Workspace/MegaGate/core/crates/mg-crypto)
- **Limit**:
  - no honest basis yet for “absolute” security claim
  - cross-language audit/integrity parity does not exist yet

## What MegaGate can honestly claim now

- native Rust-owned web dependency management core
- no npm/pnpm/bun/yarn execution in the core web install/add/remove/list/build engine
- explicit rejection of package-manager wrapper scripts in `dev`, `run`, and lifecycle paths
- strong warm-path and steady-path performance in web scenarios
- real template/scaffold support for multi-language backends

## What MegaGate cannot honestly claim yet

- faster than Bun across the board
- smarter than pnpm across the full lifecycle
- absolute security
- unified native package management for Go, Java, Python, PHP
- full cross-language memory/cache governance on one shared engine

## Required next steps

1. Build language-native adapters beyond web
   - `mg-go`
   - `mg-python`
   - `mg-java`
   - `mg-php`
2. Introduce a unified multi-ecosystem lock graph model
   - not just `mg.lock` for node-style packages
3. Implement true quarantine / freshness / policy engine
   - 24h block or stronger
   - consistent across all materializing commands
4. Harden shell-executed surfaces further
   - reduce fallback shell usage where feasible
5. Continue cold-path optimization
   - resolver metadata cost
   - streamed/parallel fetch-extract-materialize

## Bottom line

MegaGate is currently a **native web package-management core with growing multi-language project orchestration**, not yet a finished universal native package manager for every backend ecosystem.

That is still a strong base. It is just important not to call phase-1 reality a phase-final product.
