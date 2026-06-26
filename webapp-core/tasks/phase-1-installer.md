# Phase 1: Installer Orchestration — Task List

## Goal
Orchestrate: resolve → fetch → link → save lockfile.

## Files to Create
- [ ] `src/installer/index.ts` — Installer class (replace current)

## Methods
- [ ] `install(options)` — Full install
- [ ] `add(spec, options)` — Add dependency
- [ ] `update(spec?, options)` — Update dependencies
- [ ] `remove(name)` — Remove dependency
- [ ] `list(depth)` — List installed
- [ ] `verify()` — Verify lockfile integrity

## Flow (install)
```
1. Load package.json (workspace root)
2. Load lockfile (or create empty)
3. Resolver.resolve(manifests, lockfile, options)
   → ResolutionResult { lock, newPackages }
4. Security.checkReleaseAge() for each new package
5. Fetcher.fetchMultiple(newPackages)
   → Streaming download + extract to store
6. Security.runLifecycleScripts() for approved builds
7. Linker.link(cwd, lock)
8. LockfileManager.save(lock, cwd)
9. Return InstallResult
```

## Exit Codes (per SPEC)
- `0` = success
- `1` = generic error
- `2` = frozen-lockfile mismatch
- `3` = integrity verification failed
- `4` = network/registry error

## Acceptance Criteria
- [ ] `megagate install` works on real project
- [ ] `megagate add lodash` adds to package.json + lockfile
- [ ] `megagate update` updates to latest compatible
- [ ] `megagate remove lodash` removes from all
- [ ] `megagate list --depth=1` shows tree
- [ ] `megagate verify` passes on clean install
- [ ] Integration tests: full cycle

## Commands to Test
```bash
pnpm test -- tests/integration/installer.test.ts
```

## Dependencies
- All Phase 1 modules
