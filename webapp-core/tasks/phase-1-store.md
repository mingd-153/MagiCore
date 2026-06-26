# Phase 1: Store Abstraction — Task List

## Goal
Content-addressable store with pluggable backends (FS, SQLite, LMDB).

## Files to Create
- [ ] `src/store/index.ts` — `StoreBackend` interface
- [ ] `src/store/fsBackend.ts` — FS implementation (primary)
- [ ] `src/store/sqliteIndex.ts` — O(1) lookup index (stub for now)
- [ ] `src/store/types.ts` — Store-specific types

## FS Backend Layout
```
~/.megagate/store/v1/
├── files/
│   ├── pkg-name-1.0.0.tgz
│   └── pkg-name-1.0.0.tgz.sha512
└── nodes/
    └── pkg/
        └── name/
            └── 1.0.0/
                ├── package.json
                ├── node_modules/   (symlinks to deps)
                └── .megagate-meta.json
```

## Acceptance Criteria
- [ ] `store.init()` creates directories
- [ ] `store.writeTarball(stream)` → returns `{integrity, size}`
- [ ] `store.readTarball()` → readable stream
- [ ] `store.createHardlink()` / `createSymlink()` / `copy()` work
- [ ] `store.prune(referencedSet)` removes unreferenced packages
- [ ] `store.verifyIntegrity(pkg)` returns boolean
- [ ] Unit tests: all CRUD operations, link strategies, prune

## Commands to Test
```bash
pnpm test -- tests/unit/store.test.ts
```

## Dependencies
- Phase 0 (types)
- `tar-fs` for streaming extract
