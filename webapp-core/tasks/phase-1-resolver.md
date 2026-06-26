# Phase 1: Resolver — Conflict Detection & Workspace Protocol

## Goal
Version resolution with conflict detection, hoisting, and workspace:* support.

## Files to Create
- [ ] `src/resolver/graph.ts` — Dependency graph builder
- [ ] `src/resolver/conflict.ts` — Version conflict resolution
- [ ] `src/resolver/peerValidation.ts` — Peer dependency validation
- [ ] `src/resolver/workspace.ts` — workspace:* protocol + catalog:
- [ ] `src/resolver/index.ts` — Main Resolver class

## Features

### Dependency Graph
- Nodes: `name@version` with edges to dependencies
- Topological sort for fetch/link order
- Detect cycles

### Conflict Resolution
```
Conflict: pkg-a@1.0.0 needs dep@^1.0.0, pkg-b@2.0.0 needs dep@^2.0.0
Strategy:
1. If semver intersects → hoist highest compatible
2. If not → duplicate (last resort, warn)
3. Prefer lockfile versions
```

### Peer Dependency Validation
- Check each package's `peerDependencies` against resolved graph
- Report: missing, incompatible version, unmet optional
- Config: `warn` | `error` | `ignore`

### Workspace Protocol
- `workspace:*` → local package version
- `workspace:^1.0.0` → range matched against local
- `catalog:name` → version from `megagate.workspace.json catalog`

## Acceptance Criteria
- [ ] Resolves complex dependency trees correctly
- [ ] Conflicts detected and resolved per strategy
- [ ] Peer warnings/errors emitted correctly
- [ ] `workspace:*` resolves to local packages
- [ ] `catalog:` resolves to catalog versions
- [ ] Unit tests: graph, conflicts, peers, workspace, catalog

## Commands to Test
```bash
pnpm test -- tests/unit/resolver.test.ts
pnpm test -- tests/integration/resolver-monorepo.test.ts
```

## Dependencies
- Phase 0, Phase 1 Store, Security, Fetcher
