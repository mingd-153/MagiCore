# Phase 1: Linker & Lockfile — Task List

## Goal
Linking strategies + deterministic lockfile with content hash.

## Files to Create
- [ ] `src/linker/index.ts` — Strategies + main Linker
- [ ] `src/lockfile/index.ts` — LockfileManager

## Linker Strategies

### HardlinkStrategy (default, Linux/macOS)
- `store/nodes/pkg/name/version` → `.megagate/name@version` (hardlink)
- `.megagate/name@version` → `node_modules/name` (symlink)

### SymlinkStrategy (fallback)
- Both links are symlinks

### CopyStrategy (Windows fallback)
- Copy files (slow, last resort)

### Virtual Store Structure
```
project/
├── node_modules/
│   ├── .megagate/
│   │   └── pkg-name@1.0.0 -> ~/.megagate/store/v1/nodes/pkg/name/1.0.0
│   └── pkg-name -> .megagate/pkg-name@1.0.0
```

## LockfileManager
- Load/save `megagate-lock.json`
- `computeContentHash()` — SHA-256 of normalized deps for determinism
- `verifyIntegrity()` — Check all packages have valid integrity
- `migrateV1toV2()` — Future-proofing
- `export(format)` — JSON/YAML

## Acceptance Criteria
- [ ] All 3 link strategies work
- [ ] `node_modules` structure matches spec
- [ ] Lockfile contentHash changes iff deps change
- [ ] `verify` command catches corrupted packages
- [ ] Unit tests: link strategies, lockfile ops, contentHash

## Commands to Test
```bash
pnpm test -- tests/unit/linker.test.ts
pnpm test -- tests/unit/lockfile.test.ts
```

## Dependencies
- Phase 0, Phase 1 Store
