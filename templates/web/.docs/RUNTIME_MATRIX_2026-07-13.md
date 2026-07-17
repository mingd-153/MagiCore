# Web Runtime Matrix — 2026-07-13

This report captures a focused runtime verification pass for `mg/web` after the latest resolver, materialization, and Nuxt CLI fixes.

Verification shape:

- `mg create-web ...`
- `mg install-web`
- `mg dev --host 127.0.0.1 --port <port>`
- HTTP or API health check

## What Changed In This Pass

- fixed npm alias resolution in the web adapter
- fixed nested conflicting package materialization enough for Nuxt to boot
- disabled Nuxt telemetry prompt during `mg dev`
- disabled Quarkus analytics prompt during `mg dev`
- fixed shared backend JS import path:
  - `templates/web/shared/partials/backend/sources/health.js`
  - now imports `../services/status.js`
- fixed Remix template recursion:
  - `templates/web/fullstack/all-in-one/remix/sources/package.json`
  - `dev` now uses `remix vite:dev` instead of recursively calling `mg dev`
- fixed same-major resolver conflicts for web packages:
  - `core/crates/mg-resolver/src/solver/mod.rs`
  - multiple incompatible `0.x` versions can now coexist
- fixed Remix Vite target defaults:
  - `templates/web/fullstack/all-in-one/remix/sources/vite.config.ts`
  - `build`, `esbuild`, and `optimizeDeps` now target `es2022`
- fixed missing SolidJS toolchain seed:
  - `vite-plugin-solid` is now scaffolded
- fixed Angular scaffold gaps:
  - added `@angular/compiler`, `@angular/common`, and `@angular-devkit/build-angular`
  - added Angular `ignoreDeprecations: "6.0"` to `tsconfig.json`
- fixed NestJS backend template path/runtime issues:
  - corrected `config`/`routes` import paths
  - removed fragile controller constructor injection from the starter
- fixed tRPC backend seed and source paths:
  - `express`/`zod` no longer inherit `@trpc/server` version
  - `src/lib/app.ts` now imports `context` and `router` from correct locations
- fixed tar extraction for npm pax metadata:
  - `core/crates/mg-fetcher/src/extract.rs` now skips pax metadata entries instead of aborting install
- added native backend install/dev fallback in `mg web` CLI:
  - `mg install-web` now handles Go, Python, Rust, Maven, and Composer-based backend scaffolds
  - `mg dev` now detects Go/Python/Rust/Maven/PHP entrypoints even without `package.json`
- fixed Go backend scaffold layout:
  - Go `health.go` and `status.go` now materialize into the backend package instead of invalid cross-directory `package main` layouts
- fixed Django baseline compatibility:
  - `templates/web/backend/python/django/sources/requirements.txt`
  - now pins `django>=4.2,<5.0` so the default scaffold still installs on Python 3.9 environments

## Verified Cases

| Case | Result | Install Time | Runtime Check | Notes |
| --- | --- | ---: | --- | --- |
| `react-vite --ts` | Pass | `6483 ms` | `HTTP 200` on `:4353` | Vite ready quickly |
| `vue-vite --ts` | Pass | `13212 ms` | `HTTP 200` on `:4354` | runtime verified after session-based start |
| `vanilla --ts` | Pass | `6576 ms` | `HTTP 200` on `:4362` | scaffolded Vite baseline only, no placeholder failure |
| `solidjs --ts` | Pass | `8458 ms` | `HTTP 200` on `:4369` | needed missing `vite-plugin-solid` scaffold seed |
| `sveltekit --ts` | Pass | `6353 ms` | `HTTP 200` on `:4363` | Vite-based frontend path works through `mg dev` |
| `nuxt --ts` | Pass | `100154 ms` | `HTTP 200` on `:4352` | telemetry prompt removed, Nuxt/Nitro/Vite booted |
| `nextjs --ts` | Pass | `13477 ms` before fix | `HTTP 200` on `:4355` after fix | fixed by pinning scaffold TypeScript back to stable 5.x |
| `astro --ts` | Pass | `28971 ms` | `HTTP 200` on `:4364` | local Astro launcher path verified |
| `angular --ts` | Pass | `18513 ms` warm cache, `119177 ms` earlier cold-ish install | `HTTP 200` on `:4370` | required compiler/common/build-angular seeds, deprecation suppression, and non-interactive CLI env |
| `qwik --ts` | Pass | `2823 ms install, dev ready in 212 ms` | `HTTP 200` on `:4318` | fixed by pinning Vite to `^7.3.6` and switching scaffold dev script to `vite --mode ssr` |
| `remix --ts` | Pass | `70526 ms` | `HTTP 200` on `:4368` | needed template recursion fix, resolver fix, and modern Vite target |
| `fastify` | Pass | `4876 ms` fresh, `931 ms` cached rerun | `GET /health -> 200` on `:4358` | required shared backend import fix |
| `express --ts` | Pass | `8172 ms` | `GET /health -> 200` on `:4365` | standalone backend runtime verified |
| `hono --ts` | Pass | `5012 ms` | `GET /health -> 200` on `:4374` | required `@hono/node-server` plus real listener bootstrap |
| `nestjs --ts` | Pass | `10624 ms` | `GET /health -> 200` on `:4372` | required pax tar fix plus starter runtime cleanup |
| `trpc --ts` | Pass | `2068 ms` | `GET /health -> 200` on `:4373` | required primary/supplemental seed correction and source path fix |
| `react-express --ts` | Pass | `5616 ms` frontend + `830 ms` backend | frontend `HTTP 200` on `:4366`, backend `GET /health -> 200` on `:4367` | split fullstack FE/BE routing verified |
| `echo` | Pass | native `mg install-web` + `go mod tidy` | `GET /health -> 200` on `:4387` | required Go scaffold layout fix and native install flow |
| `gin` | Pass | native `mg install-web` + `go mod tidy` | `GET /health -> 200` on `:4390` | runtime verified after Go scaffold layout fix |
| `fiber` | Pass | native `mg install-web` + `go mod tidy` | `GET /health -> 200` on `:4391` | runtime verified after Go scaffold layout fix |
| `axum` | Pass | native `mg install-web` + `cargo fetch` | `GET /health -> 200` on `:4386` | runtime verified after native dev fallback |
| `actix-web` | Pass | native `mg install-web` + `cargo fetch` | `GET /health -> 200` on `:4393` | compile-heavy first boot, then healthy |
| `fastapi` | Pass | native `mg install-web` + `.venv` bootstrap | `GET /health -> 200` on `:4382` | runtime verified using MegaGate-managed virtualenv |
| `flask` | Pass | native `mg install-web` + `.venv` bootstrap | `GET /health -> 200` on `:4392` | runtime verified using MegaGate-managed virtualenv |
| `spring-boot` | Pass | native `mg install-web` + `mvn dependency:go-offline` | `GET /health -> 200` on `:4321` | runtime verified after local Maven toolchain install |
| `quarkus` | Pass | native `mg install-web` + Maven first-run downloads | `GET /health -> 200` on `:4325` | fixed missing Quarkus Maven plugin/build goals; `mg dev` launcher now disables analytics prompt |
| `laravel` | Pass | native `mg install-web` + `composer install` | `GET /health -> 200` on `:4323` | fixed outdated framework constraint, artisan bootstrap, runtime directories, `public/index.php`, default app key, and file-session config |
| `symfony` | Pass | native `mg install-web` + `composer install` | `GET /health -> 200` on `:4324` | fixed invalid `composer.json`, added `Kernel`, framework router config, `symfony/string`, and native `mg dev` fallback via `php -S` |
| `django` | Pass | native `mg install-web` + `.venv` bootstrap | `GET /health -> 200` on `:4394` | required compatibility pin back to Django 4.2 for Python 3.9 |
| `react-vite --monorepo --backend fastify --ts` | Pass | `7694 ms` frontend + `257 ms` package | frontend `HTTP 200` on `:4384`, backend `GET /health -> 200` on `:4385` | requested port override now propagates correctly |
| `react-vite --monorepo --backend fastapi --ts` | Pass | `8278 ms` frontend + native Python bootstrap | frontend `HTTP 200` on `:4395`, backend `GET /health -> 200` on `:4396` | verified that monorepo native backends are no longer skipped by `mg install-web` |

## Still Red Or Under Re-Validation

| Case | Current Read | Notes |
| --- | --- | --- |
| broader language lanes | Partial | core Java/PHP lanes are now runtime-verified; remaining debt is breadth across more framework combinations and cold-path cost |

## Important Findings

### 1. Nuxt is now genuinely green

Nuxt previously failed in three places across recent iterations:

- npm alias packages
- multi-version nested dependencies
- first-run telemetry prompt

This pass verified that:

- `mg install-web` completes
- `mg dev` starts without asking the user anything
- `curl -I http://127.0.0.1:4352` returns `200`

That is a real end-to-end improvement.

### 2. Next.js root cause was scaffold drift, and it is now fixed

The breakage was not a generic installer failure.

Actual cause:

- scaffold seeded `typescript@7.x`
- Next checks specifically for `typescript/lib/typescript.js`
- TypeScript 7 no longer exposes the same package layout
- Next treated TypeScript as missing and fell back to `pnpm` auto-install

Fix:

- scaffold now prefers MegaGate baseline versions before registry latest for toolchain packages
- `nextjs --ts` now seeds `typescript: ^5.9.2`
- runtime check now returns `HTTP 200`

### 3. Shared backend partial bug was real

Fastify failed on Node 26 because:

- `health.js` imported `../services/status`
- ESM runtime required `../services/status.js`

Fixing the shared partial fixed the Fastify runtime path and should also help any backend template reusing the same JS partial.

### 4. Monorepo runtime is now aligned with CLI port intent

The monorepo case booted both targets correctly:

- frontend on requested `:4360`
- backend on `:4361`

This pass confirmed CLI target selection in a real rerun:

- frontend uses `X`
- backend uses `X + 1`

That is a better product default for multi-target local dev.

### 5. Remix exposed two different classes of defect, both now fixed

First failure:

- template root `package.json` used `"dev": "mg --core web dev"`
- `mg dev` then re-read that script and failed with unsupported recursion

Second failure:

- Remix required conflicting `esbuild` versions in the graph
- the resolver previously collapsed incompatible same-major `0.x` versions
- Vite then picked the wrong root `esbuild` in the dev path and exploded on old browser targets

Fix set:

- `remix` template now launches via `remix vite:dev`
- resolver now preserves incompatible same-major versions instead of replacing them
- Remix Vite config now targets `es2022` for `build`, `esbuild`, and `optimizeDeps`

After those changes:

- `mg install-web` writes both `esbuild@0.17.6` and `esbuild@0.28.1` into `mg.lock`
- nested `@remix-run/dev/node_modules/esbuild` is materialized correctly
- `mg dev --host 127.0.0.1 --port 4368` returns `HTTP 200`

## Product Readiness Read

### Green enough right now

- `react-vite`
- `vue-vite`
- `vanilla`
- `angular`
- `sveltekit`
- `nuxt`
- `astro`
- `nextjs`
- `solidjs`
- `qwik`
- `remix`
- `fastify`
- `express`
- `hono`
- `nestjs`
- `trpc`
- `echo`
- `gin`
- `fiber`
- `axum`
- `actix-web`
- `fastapi`
- `flask`
- `django`
- `react-express`
- `react-vite + fastify monorepo` baseline
- `react-vite + fastapi monorepo` baseline

### Not green enough yet

- broader framework matrix still incomplete

## What Still Needs Work

1. Benchmark cold install and repeated install more systematically
2. Expand the same runtime matrix to:
   - more split fullstack pairs
   - more monorepo backend combinations

## Practical Conclusion

`mg/web` is no longer in the state where everything is just scaffold-pretty but runtime-fragile.

There is now a real green lane for several important templates.

But it is not ready to claim full product competitiveness yet because:

- cold install on heavy graphs like Nuxt is still expensive
- runtime coverage is still not broad enough across every supported template
- some cold online first-run paths are still much heavier than the Bun/pnpm bar, especially Quarkus/Nuxt-class graphs
