# Phase 0: Types & Config — Task List

## Goal
Foundation types and config system. No external deps.

## Files to Create
- [ ] `src/types/index.ts` — All core interfaces
- [ ] `src/config/index.ts` — TOML parser + loader
- [ ] `src/config/schema.ts` — Validation schemas

## Acceptance Criteria
- [ ] `import { MegagateConfig, PackageManifest, LockfileV1 } from './types'` works
- [ ] `megagate.toml` + `~/.megagaterc` load correctly
- [ ] Env var overrides work (`MEGAGATE_STORE_DIR`, `MEGAGATE_REGISTRY`)
- [ ] Unit tests pass: config loading, validation, defaults

## Commands to Test
```bash
pnpm test -- tests/unit/config.test.ts
pnpm test -- tests/unit/types.test.ts
```

## Dependencies
- None (stdlib only)
