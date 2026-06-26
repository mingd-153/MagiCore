# Phase 1: Security Module — Task List

## Goal
Security-first defaults: minimumReleaseAge, approve-builds, lockdown mode.

## Files to Create
- [ ] `src/security/index.ts` — `SecurityManager` class
- [ ] `src/security/approveBuilds.ts` — Lifecycle script approval
- [ ] `src/security/minimumReleaseAge.ts` — 24h block
- [ ] `src/security/lockdown.ts` — Vanilla project hardening
- [ ] `src/security/types.ts` — Security types

## Features

### minimumReleaseAge (default 24h)
- Check `publishTime` from registry
- Block if `now - publishTime < 24h`
- Config: `megagate.toml [security] minimumReleaseAgeHours`
- Flag: `--ignore-minimum-age`

### approve-builds (default ON)
- Deny ALL lifecycle scripts by default (`prepare`, `preinstall`, `postinstall`, `prepublish`)
- Allowlist stored in lockfile: `approvedBuilds: string[]`
- CLI: `megagate security approve-builds <pkg> [--script postinstall]`
- Run in sandbox: no network, no fs outside cwd, 60s timeout

### lockdownMode (default OFF, ON for vanilla templates)
- Scan for native addons (`.node`, `binding.gyp`)
- Static AST check: no `eval`, `Function`, `new Function`
- Enforce `sideEffects: false` in package.json
- CSP-compatible output

## Integration Points
- Resolver: call `checkReleaseAge()` before adding to graph
- Fetcher: call `checkBuildApproval()` before running scripts
- Installer: call `validateLockdown()` for lockdown projects

## Acceptance Criteria
- [ ] Unit tests: each check passes/fails correctly
- [ ] Integration: `megagate install` blocks new packages
- [ ] CLI: `approve-builds` adds to lockfile, allows script
- [ ] Lockdown template gets extra validation

## Commands to Test
```bash
pnpm test -- tests/unit/security.test.ts
pnpm test -- tests/integration/security.test.ts
```

## Dependencies
- Phase 0, Phase 1 Store
