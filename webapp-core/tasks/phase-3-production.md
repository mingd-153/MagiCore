# Phase 3: Production Hardening — Task List

## Goal
Production-ready: single binary, migration tools, diagnostics, Rust acceleration.

## Files to Create

### Migration Tools
- [ ] `src/cli/commands/migrate.ts`
  - `migrate from-pnpm`
  - `migrate from-npm`
  - `migrate from-yarn`
  - `migrate from-bun`

### Diagnostics
- [ ] `src/cli/commands/doctor.ts` — Environment health check
- [ ] `src/cli/commands/audit.ts` — Vulnerability audit (OSV)
- [ ] `src/cli/commands/why.ts` — Reverse dependency graph
- [ ] `src/cli/commands/outdated.ts` — Check for updates

### Single Binary (Node.js SEA)
- [ ] `scripts/buildSea.ts` — Build single executable
- [ ] `package.json` — Add `sea` build script

### Rust Acceleration (napi-rs) — Optional v1.1
- [ ] `packages/core-native/` — Rust crate
  - `compute_integrity`
  - `resolve_versions`
  - `link_package`
- [ ] `src/native/` — TypeScript bindings

## Acceptance Criteria

### Migration
- [ ] `megagate migrate from-pnpm` converts lockfile correctly
- [ ] All 4 migration paths work
- [ ] Preserves versions, integrity, dependencies

### Doctor
- [ ] Checks: Node version, store, lockfile, peers, disk, network, config
- [ ] Exit code 0 = healthy, 1 = issues

### Audit
- [ ] Queries OSV/GitHub Advisory
- [ ] Reports: severity, CVE, fixed version
- [ ] Exit codes: 0=none, 1=low, 2=moderate, 3=high, 4=critical

### Why
- [ ] Shows reverse deps: direct, transitive, workspace
- [ ] Explains version selection

### Single Binary
- [ ] `./megagate` runs without Node.js installed
- [ ] Cross-platform: Linux, macOS, Windows (x64 + ARM64)

### Rust (if implemented)
- [ ] 3-10x speedup on hot paths
- [ ] Benchmarks documented

## Commands to Test
```bash
pnpm test -- tests/integration/migrate.test.ts
pnpm test -- tests/integration/doctor.test.ts
pnpm test -- tests/integration/audit.test.ts
./megagate --version  # single binary
```

## Dependencies
- Phase 1, 2 complete
- New deps: `@osv/osv`, `napi-rs` (for Rust)
