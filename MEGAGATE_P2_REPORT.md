# MegaGate P2 — Fullstack Template & Codebase Cleanup

## Overview

Phase 2: xây dựng fullstack web template (`fullstack/split/react-fastify`), mở rộng coverage lên nextjs/vue-vite/sveltekit, dọn dẹp toàn bộ codebase web templates. **26/26 `cargo test -p mg` pass**, 6 scaffold variants verified.

---

## 1. Fullstack Template: `fullstack/split/react-fastify`

**Created from scratch** — template cho fullstack React + Fastify split architecture.

### Structure
```
sources/
├── root/package.json        # Root: scripts dev:client/server, concurrently
├── client/                  # React Vite frontend
│   ├── App.{tsx,jsx}        # Standalone file (fastify-specific content)
│   ├── main.{tsx,jsx}       # Symlink → frontend/react-vite
│   ├── vite.config.{ts,js}  # Standalone file (port 4315, proxy /api → :3000)
│   └── ...                  # tsconfig, index.html, etc. (symlinks)
├── server/                  # Fastify backend (symlinks → backend/node/fastify/)
│   └── package.{ts,js}.json # Standalone file (name: {{project_slug}}-backend)
└── styles/, assets/         # Symlinks → shared partials
```

### Key changes
- `template.toml`: 23 file entries, `required_context` cho `project_slug` + `project_name`
- `cli/src/commands/core/web.rs`: Added `fullstack_backend_framework()`, `resolve_seed_name()`, updated `enrich_web_project_manifest` to dual-seed (root + server)
- Frontend deps seed vào root `package.json`, backend deps vào `server/package.json`
- Vite proxy `/api` → `http://localhost:3000`

---

## 2. Bug Fixes

### 2.1 Shared App.tsx Content (HIGH)
- **Issue**: `frontend/react-vite/sources/App.tsx` hiển thị "react + fastify" — sai cho standalone/monorepo
- **Fix**: Generic React content, dùng `{{project_name}}` token

### 2.2 Fullstack App.tsx Missing (HIGH)
- **Issue**: `fullstack/split/react-fastify` symlink đến frontend — nhận content generic
- **Fix**: Tạo file riêng `App.{tsx,jsx}` với content "react + fastify" + "Fullstack-ready, Fastify-powered"

### 2.3 main.jsx TS Syntax (HIGH)
- **Issue**: `main.jsx` có `as HTMLElement` (TypeScript-only) → JS variant parse error
- **Fix**: Remove type assertion → `document.getElementById("root")`

### 2.4 Broken Router Imports (MEDIUM)
- **Issue**: `router/AppRouter.{tsx,jsx}` import `AppShell` + `engineBridge` không tồn tại
- **Fix**: Replace với self-contained placeholder component

### 2.5 Empty components/ Directory (LOW)
- **Issue**: `frontend/react-vite/sources/components/` empty
- **Fix**: Removed

### 2.6 Monorepo Workspace Name Conflict (HIGH)
- **Issue**: Root + child `package.json` cùng `name: "{{project_slug}}"` → npm `EDUPLICATEWORKSPACE`
- **Fix**: 12 files changed — `{{project_slug}}` → `{{project_slug}}-frontend` / `-backend`
  - 4 monorepo frontends (react-vite, vanilla, vue-vite, solidjs × JS+TS)
  - 1 monorepo backend (fastify × JS+TS)
  - 1 fullstack server (react-fastify × JS+TS)
  - (sveltekit đã có `-frontend` từ trước)

### 2.7 Orphaned Duplicate Files (MEDIUM)
- **Issue**: `shared/partials/frontend-common` + `monorepo-frontend-common` chứa 8 files mỗi cái, là bản sao của `frontend-foundation`, không được `template.toml` reference
- **Fix**: Removed 16 files (config/brand, hooks/useProjectLinks, favicon, logo, svg, theme.css)
- **Follow-up**: Fullstack template symlinks đến `frontend-common` cho assets → repointed → `frontend-foundation`

### 2.8 Stale `required_context` Declarations (LOW)
- **Issue**: 6 templates khai báo `required_context = ["project_name"]` cho files không dùng `{{project_name}}`
- **Fix**: 11 entries cleaned across:
  - `frontend/nextjs` (page.jsx/tsx)
  - `frontend/react-vite` (main.jsx/tsx)
  - `frontend/sveltekit` (app.html, +layout.svelte)
  - `backend/node/fastify` (server.js/ts, app.js/ts)
  - `monorepo/frontend/react-vite` (main, config/framework)
  - `monorepo/frontend/sveltekit` (app.html, +layout.svelte)

### 2.9 SvelteKit TypeScript Version Conflict (HIGH)
- **Issue**: `@sveltejs/kit@^2.69.2` peer-dep `typescript@^5.3.3 || ^6.0.0` nhưng seed dùng `^7.0.2` → npm ERESOLVE
- **Fix**: Added `version: Option<&str>` to `WebToolchainPackage` struct. SvelteKit seed overrides typescript → `^6.0.3`
  ```rust
  WebToolchainPackage {
      section: "devDependencies",
      package: "typescript",
      typescript_only: true,
      version: Some("^6.0.3"),  // override global ^7.0.2
  },
  ```

### 2.10 Next.js Turbopack Root + Script Fixes (HIGH)
- `next.config.mjs`: Thêm `turbopack: { root: __dirname }` fix root inference
- `layout.tsx`: Thêm metadata, `import "../styles/theme.css"`
- `package.json`: Scripts từ `mg web dev` → `next dev`
- `template.toml`: Thêm `include_features`/`exclude_features`, JS/TS variant files, config/framework

### 2.11 React-Vite Template JSX Migration (MEDIUM)
- **Migration**: `App.ts`/`main.ts` (plain TS) → `App.tsx`/`main.tsx` + `App.jsx`/`main.jsx` (JS variants)
- `vite.config.ts`: Thêm `plugins: [react()]` — thiếu trước đó
- `template.toml`: Thêm `include_features`/`exclude_features` cho feature gating (typescript)

---

## 3. Test Results

```
cargo test -p mg — 26 passed, 0 failed
(2 tests mới: test_dev_targets_for_fullstack_include_frontend_and_backend,
               test_install_targets_cover_fullstack_and_monorepo_children)

6 scaffold variants verified:
├── standalone react-vite    → npm install, Vite :5173  ✅
├── sveltekit                → npm install, Vite :5173  ✅ (fixed TS version)
├── nextjs                   → scaffold ✅ (npm install slow ~69s)
├── vue-vite                 → npm install ✅
├── monorepo react-vite      → npm install ✅
└── fullstack react-fastify  → npm install, Vite :4315  ✅
```

### Package.json names after fix
| Template | Root | Frontend | Backend |
|----------|------|----------|---------|
| standalone | `{{project_slug}}` | — | — |
| monorepo | `{{project_slug}}` | `{{project_slug}}-frontend` | `{{project_slug}}-backend` |
| fullstack | `{{project_slug}}` | — | `{{project_slug}}-backend` |

---

## 4. Architectural Changes

### 4.1 `WebToolchainPackage.version` Field
New optional field cho phép framework seed override version 1 package cụ thể thay vì dùng global baseline:
```rust
struct WebToolchainPackage {
    section: &'static str,
    package: &'static str,
    typescript_only: bool,
    version: Option<&'static str>,  // NEW: per-package version override
}
```
Hiện tại sveltekit dùng `version: Some("^6.0.3")` cho typescript.

### 4.2 `BuildShape` + `available_core_names()`
- `factory.rs`: Thêm `BuildShape` enum (`SingleCore` / `MultiCore`), `is_single_core_build()`
- `context.rs`: Dùng `available_core_names()` thay vì `available_cores()` trực tiếp

---

---

## 5. Phase 3 — Benchmark + Visual QA + Audit

### 5.1 Benchmark Chính Thức (criterion)

**4 benchmarks on criterion, synthetic tarballs (no network), repeatable:**

| Bench | Scenarios | Output |
|---|---|---|
| `cold_path` | 3 install benches (small/medium/real) | criterion table |
| `stress` | 7 scenarios (large_tree, concurrent, corrupted, deep_chain, reinstall, mixed_integrity, clean_reinstall) | criterion table |
| `install_bench` | 5 install strategies with seed_cached_tarball | criterion table |
| `compare` | MegaGate vs npm/pnpm/bun real-world | comparison table |

**Infrastructure:**
- `scripts/bench.sh` — local runner: `quick`, `all`, `cold`, `stress`, `install`, `compare`, `baseline`, `diff`
- CI: benchmark job chạy trên `push → main` với `--save-baseline main`, upload artifact
- Criterion baseline support: `--save-baseline` / `--load-baseline`

### 5.2 Trust Model

| Check | Status | Config |
|---|---|---|
| `cargo deny check` | ✅ advisories ok, bans ok, licenses ok, sources ok | `deny.toml` — ignored 4 tracked advisories, banned multi-versions (skipped transitive), license allowlist |
| `cargo audit` | ✅ runs in CI | `--deny warnings` with ignored advisories |
| **CI step** | ✅ added to `ci.yml` | `cargo deny check` + `cargo audit --deny warnings` |

**Tracked advisories (ignored, no fix available):**
- `RUSTSEC-2025-0141` — bincode unmaintained (direct dep in mg-store)
- `RUSTSEC-2025-0119` — number_prefix unmaintained (transitive via indicatif)
- `RUSTSEC-2024-0436` — paste unmaintained (transitive via ratatui)
- `RUSTSEC-2026-0002` — lru unsound (used in mg-store + ratatui)

### 5.3 Visual QA (Playwright)

- `cli/tests/visual_qa.rs` — Rust integration test, disabled by default (`#[ignore]`)
- Run: `cargo test --test visual_qa -- --ignored`
- Scaffolds project → `npm install` → dev server → Playwright screenshot + console error check
- Headless Chromium, 1280×720 viewport
- Requires: node + npx playwright installed

---

## 6. Remaining Known Issues

### 6.1 Unimplemented Templates (40 stubs)
- 4 fullstack registered frameworks: `remix`, `react-spring`, `vue-laravel`, `custom`
- All-in-one templates (nextjs, nuxt, sveltekit)
- Backend frameworks (express, nestjs, laravel, spring-boot, django, gin, axum...)

### 6.2 Next.js AppShell EngineBridge Dependencies
- `page.tsx`/`page.jsx` imports `../components/AppShell` + `../bridges/engine` — hiện đã có nhờ shared partials trong scaffold output, nhưng phụ thuộc vào cấu trúc partials

### 6.3 Tracked Advisory Fixes
- Replace `bincode` with `bincode2` or `rmp-serde` in mg-store
- Update `indicatif` / `ratatui` when upstream releases fix for number_prefix + paste
- Fix `lru` violation (or replace with hashlink)

---

## 7. Files Changed (58+ files, +2476 / -1689 → 67+ files, +~2700 / -~1700)

```
cli/src/
├── commands/core/web.rs            [EDIT] FRAMEWORK_SEEDS version override, fullstack mapping, nextjs fixes
├── commands/mod.rs                 [EDIT] module restructure (remove old create/install/list/remove/update)
├── context.rs                      [EDIT] use is_single_core_build()
├── factory.rs                      [EDIT] BuildShape, available_core_names()
├── main.rs                         [EDIT] major CLI restructure
└── scaffold/processor.rs           [EDIT] WEB_FRAMEWORKS constant, template validation

templates/web/
├── fullstack/split/react-fastify/  [NEW] template.toml + 23 sources
├── frontend/react-vite/            [EDIT] App.tsx/jsx, main.jsx, vite.config.ts, template.toml
├── frontend/nextjs/                [EDIT] next.config.mjs, layout.tsx, page.tsx, package.json, tsconfig.json, template.toml
├── frontend/sveltekit/             [EDIT] template.toml (remove stale required_context)
├── backend/node/fastify/           [EDIT] template.toml, server.ts, tsconfig.json
├── monorepo/frontend/react-vite/   [EDIT] template.toml, package names
├── monorepo/frontend/vanilla/      [EDIT] package names
├── monorepo/frontend/vue-vite/     [EDIT] package names
├── monorepo/frontend/solidjs/      [EDIT] package names
├── monorepo/frontend/sveltekit/    [EDIT] template.toml
├── monorepo/backend/node/fastify/  [EDIT] template.toml, server.ts, tsconfig.json, package names
└── shared/partials/                [EDIT] frontend-common + monorepo-frontend-common (orphaned files)

adapters/web/benches/
├── cold_path.rs                    [REWRITE] criterion, synthetic data, 3 install benches
├── stress.rs                       [REWRITE] criterion, 7 scenarios

scripts/
└── bench.sh                        [NEW] local benchmark runner (cold/stress/install/compare/quick/all)

cli/tests/
└── visual_qa.rs                    [NEW] Rust integration test, scaffold → dev server → Playwright

deny.toml                           [NEW] cargo-deny config: advisories, bans, licenses, sources
.github/workflows/ci.yml            [EDIT] added benchmark job + deny + audit steps
```
