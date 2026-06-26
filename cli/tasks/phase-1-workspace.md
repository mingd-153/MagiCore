# Phase 1: Workspace / Monorepo — Task List

## Goal
First-class monorepo support: workspace config, filter, catalog, recursive commands.

## Files to Create
- [ ] `src/workspace/config.ts` — megagate.workspace.json loader
- [ ] `src/workspace/discover.ts` — Package discovery
- [ ] `src/workspace/protocol.ts` — workspace:* resolver
- [ ] `src/workspace/catalog.ts` — Version catalog
- [ ] `src/workspace/filter.ts` — --filter selector grammar

## Workspace Config (megagate.workspace.json)
```json
{
  "packages": ["packages/*", "apps/*"],
  "catalog": { "react": "^18.2.0", "typescript": "^5.3.0" },
  "overrides": { "lodash": "4.17.21" },
  "linkWorkspacePackages": "deep"
}
```

## Filter Selector Grammar
- `--filter=@scope/*` — all packages in scope
- `--filter=./packages/foo` — specific package
- `--filter=...^foo` — foo + dependents
- `--filter=foo^...` — foo + dependencies
- `--filter="[origin/main]"` — changed since branch

## Recursive Commands
- `megagate -r install` — install in all packages
- `megagate workspace run build --filter=@scope/*` — run script in filtered

## Acceptance Criteria
- [ ] Discovers packages from globs
- [ ] `workspace:*` resolves to local versions
- [ ] `catalog:react` resolves to catalog version
- [ ] `--filter` selects correct packages
- [ ] `-r` runs command in all packages (topological order)
- [ ] Integration test: 5-package monorepo

## Commands to Test
```bash
pnpm test -- tests/integration/workspace.test.ts
```

## Dependencies
- Phase 1 Resolver, Installer, CLI
