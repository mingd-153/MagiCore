# Phase 4: Ecosystem & Polish — Task List

## Goal
Ecosystem completeness: provenance, SBOM, plugins, docs, benchmarks.

## Files to Create

### Security Advanced
- [ ] `src/security/provenance.ts` — Sigstore/SLSA verification
- [ ] `src/security/sbom.ts` — SPDX/CycloneDX generation
- [ ] `src/security/license.ts` — License compliance checker

### Plugin System
- [ ] `src/core/plugins.ts` — PluginManager + hooks

### Documentation
- [ ] `docs/` — VitePress site
  - Getting Started
  - Configuration
  - CLI Reference
  - Migration Guides
  - Architecture

### Benchmarks
- [ ] `benchmarks/run.ts` — Automated benchmark suite
- [ ] CI integration for regression detection

## Acceptance Criteria

### Provenance
- [ ] Verifies sigstore signatures
- [ ] Generates attestations for publish

### SBOM
- [ ] Outputs SPDX 2.3 + CycloneDX 1.5
- [ ] Includes: name, version, license, copyright, homepage

### License
- [ ] Checks against allowlist
- [ ] Reports violations

### Plugins
- [ ] Hooks: pre-install, post-install, pre-fetch, transform, resolve
- [ ] Load from `megagate.toml [plugins]`

### Docs
- [ ] Builds and deploys
- [ ] Search works
- [ ] All CLI commands documented

### Benchmarks
- [ ] Runs: cold install, warm install, monorepo, dev server, build, test
- [ ] CI fails on >10% regression

## Commands to Test
```bash
pnpm test -- tests/unit/provenance.test.ts
pnpm test -- tests/unit/sbom.test.ts
pnpm run bench
pnpm run docs:build
```

## Dependencies
- Phase 1-3 complete
- New deps: `@sigstore/verify`, `spdx-js`, `license-checker`
