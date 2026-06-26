# Phase 2: Dev Toolchain (Runtime) — Task List

## Goal
Bun-like DX: dev server, bundler, test runner, TS executor — all built-in.

## Files to Create
- [ ] `src/runtime/tsExecutor.ts` — TypeScript executor (zero-config)
- [ ] `src/runtime/devServer.ts` — Dev server + HMR
- [ ] `src/runtime/bundler.ts` — Production bundler (esbuild wrapper)
- [ ] `src/runtime/testRunner.ts` — Test runner (Vitest-compatible)
- [ ] `src/runtime/templates/` — Project templates

## Features

### TS Executor
- `megagate exec script.ts` — runs TS directly
- Type-stripping via oxc or TypeScript Compiler API
- Cache: `~/.megagate/ts-cache/` (content-hash based)
- Supports: TS, JSX, .mjs/.cjs detection

### Dev Server
- `megagate dev` — starts dev server
- File watcher (chokidar)
- HMR via WebSocket
- Transform on-demand (no full bundle)
- HTML entry point support
- Proxy API for backend

### Bundler
- `megagate build` — production build
- esbuild under the hood (Go binary, fast)
- Code splitting, tree shaking
- CSS handling (modules, extract, PostCSS)
- Asset handling (images, fonts)
- Sourcemaps

### Test Runner
- `megagate test` — runs tests
- Vitest-compatible API (`describe`, `it`, `expect`, `vi.fn`)
- Snapshot testing
- Coverage via v8
- Parallel execution
- Watch mode

### Templates
```
templates/
├── vanilla/      # HTML + TS + CSS (lockdown mode)
├── react/
├── vue/
├── svelte/
├── solid/
├── node-lib/     # Library mode
└── worker/       # Cloudflare Workers
```

## Acceptance Criteria
- [ ] `megagate exec` runs TS file with imports
- [ ] `megagate dev` starts server with HMR
- [ ] `megagate build` outputs production bundle
- [ ] `megagate test` runs test files
- [ ] `megagate init` scaffolds from template
- [ ] All templates work

## Commands to Test
```bash
pnpm test -- tests/integration/runtime.test.ts
```

## Dependencies
- Phase 1 complete
- New deps: `esbuild`, `chokidar`, `ws`, `oxc` (or `typescript`), `vitest` (core)
