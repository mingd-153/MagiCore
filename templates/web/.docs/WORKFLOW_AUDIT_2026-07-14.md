# Workflow Audit — 2026-07-14

This pass re-checked the real `mg/web` workflow shape in the exact order:

1. `mg create`
2. `mg install`
3. `mg add`
4. `mg remove`
5. `mg dev`

The goal of this pass was not broad marketing coverage. It was to expose root correctness gaps in real generated projects.

## What Was Fixed In This Pass

### 1. Node backend manifests were invalid for re-install

Problem:

- standalone Node backend templates and monorepo Node backend templates were emitting `package.json` without `"version"`
- `mg install` then failed while reading the generated project again

Fix:

- added `"version": "0.1.0"` to Node backend template package manifests
- updated the backend generator so future regeneration keeps the field

Affected lane:

- `templates/web/backend/node/*`
- `templates/web/monorepo/backend/node/*`
- `scripts/gen-backend-templates.py`

### 2. Fastify backend starter was structurally wrong

Problem:

- generated backend imported `./lib/app.js` while the template actually materialized `src/app.ts`
- Fastify starter source was still Express-shaped, so `mg dev` failed or routed incorrectly

Fix:

- regenerated Fastify backend starter to use:
  - `src/app.ts` / `src/app.js`
  - `import Fastify from "fastify"`
  - `await app.listen({ host: "0.0.0.0", port })`
  - `GET /health`

Affected lane:

- `templates/web/backend/node/fastify/*`
- `templates/web/monorepo/backend/node/fastify/*`
- `templates/web/fullstack/split/*-fastify/*`
- `scripts/gen-backend-templates.py`
- `scripts/gen-fullstack-templates.py`

## Real Workflow Results

Shared cache root used during this pass:

- `/private/tmp/mg-workflow-smoke4.zkDZBS/shared-cache`

### A. Standalone backend: `fastify --ts`

Project:

- `/private/tmp/mg-workflow-smoke4.zkDZBS/fastify-be`

Result:

- `mg create fastify fastify-be --ts` ✅
- `mg install` ✅
- `mg add tiny-invariant` ✅
- `mg remove tiny-invariant` ✅
- `mg dev --host 127.0.0.1 --port 4326` ✅
- `GET /health -> 200` ✅
- `mg.lock.sha256` verified ✅

Measured data:

- install time: `20.398s`
- node_modules files: `2433`
- node_modules bytes: `45310837`

### B. Split fullstack: `react-fastify --ts`

Project:

- `/private/tmp/mg-workflow-smoke4.zkDZBS/react-fastify-full`

Result:

- `mg create react-fastify react-fastify-full --ts` ✅
- `mg install` ✅
- `mg add tiny-invariant` ✅
- `mg remove tiny-invariant` ✅
- `mg dev --host 127.0.0.1 --port 4317` ✅
- frontend `GET / -> 200` ✅
- `mg.lock.sha256` verified at root ✅

Measured data:

- root install phase: `12.638s`
- backend install phase: `1.590s` with cache reuse
- root node_modules files: `560`
- root node_modules bytes: `64722659`

### C. Monorepo: `react-vite --monorepo --fastify --ts`

Project:

- `/private/tmp/mg-workflow-smoke4.zkDZBS/react-mono`

Result:

- `mg create react-vite react-mono --monorepo --fastify --ts` ✅
- `mg install` ✅
- `mg add tiny-invariant` ✅
- `mg remove tiny-invariant` ✅
- `mg dev --host 127.0.0.1 --port 4319` ✅
- frontend `GET / -> 200` ✅
- root `mg.lock.sha256` verified ✅
- child lock hashes verified:
  - `apps/frontend` ✅
  - `apps/backend` ✅
  - `packages/contracts` ✅

Observed layout:

- root has no materialized `node_modules` payload of its own
- install is distributed across workspace children
- unified root lock exists and validates

## What This Pass Proves

1. The Fastify lane is healthier than before across:
   - standalone backend
   - split fullstack
   - monorepo backend pairing
2. `mg.lock` sidecar hashing is real and verifiable in generated outputs.
3. Shared-cache reuse is visible in later installs.
4. The previous “backend Node scaffold cannot survive its own install/dev loop” bug was real and is now fixed for the Fastify lane.

## What Is Still Not Good Enough

This pass also exposed a larger structural debt:

- several non-Fastify Node backend families still inherit Express-shaped starter logic from shared generators
- that means broad claims like “all backend frameworks are production-ready” are still too optimistic

In practical terms:

- Fastify got a real workflow repair in this pass
- the wider Node backend family still needs the same framework-specific regeneration treatment:
  - Hono
  - tRPC
  - NestJS
- broader fullstack variants that depend on those backend families should be re-run after that regeneration

## Product Readiness Read After This Pass

### Stronger than before

- Fastify backend path
- React + Fastify split path
- React + Fastify monorepo path
- lock/hash verification story

### Still not ready to call fully product-ready

Because these are still open:

1. cold online path is still expensive on heavier graphs
2. broad backend family parity is not yet re-proven after generator drift
3. the exact `create -> install -> add -> remove -> dev` lane has not yet been re-run across every advertised framework in this new stricter harness
4. cross-platform installability is still not validated here:
   - this pass only ran on macOS
   - Windows and Linux support should not be claimed as verified without CI or local runtime evidence

## Immediate Next Step

Run the same strict workflow lane next for:

1. `nestjs`
2. representative native backends where `mg add/remove` semantics need to be defined explicitly instead of guessed

---

## Extension Pass — Hono + tRPC

After the Fastify repair, the same strict workflow lane was extended to Hono and tRPC.

### D. Standalone backend: `hono --ts`

Project:

- `/private/tmp/mg-workflow-smoke6.A81viY/hono-be`

Result:

- `mg create hono hono-be --ts` ✅
- `mg install` ✅
- `mg add tiny-invariant` ✅
- `mg remove tiny-invariant` ✅
- `mg dev --host 127.0.0.1 --port 4332` ✅
- `GET /health -> 200` ✅

Measured data:

- install time: `13.914s`

### E. Standalone backend: `trpc --ts`

Project:

- `/private/tmp/mg-workflow-smoke7.XuMGxY/trpc-be`

Result:

- `mg create trpc trpc-be --ts` ✅
- `mg install` ✅
- `mg add tiny-invariant` ✅
- `mg remove tiny-invariant` ✅
- `mg dev --host 127.0.0.1 --port 4341` ✅
- `GET /health -> 200` ✅

Measured data:

- install time: `19.231s`

### F. Split fullstack: `react-hono --ts`

Project:

- `/private/tmp/mg-workflow-smoke5.QQtqr8/react-hono-full`

Result:

- `mg create react-hono react-hono-full --ts` ✅
- `mg install` ✅
- `mg add tiny-invariant` ✅
- `mg remove tiny-invariant` ✅
- `mg dev --host 127.0.0.1 --port 4329` ✅
- frontend `GET / -> 200` ✅

### G. Split fullstack: `react-trpc --ts`

Project:

- `/private/tmp/mg-workflow-smoke5.QQtqr8/react-trpc-full`

Result:

- `mg create react-trpc react-trpc-full --ts` ✅
- `mg install` ✅
- `mg add tiny-invariant` ✅
- `mg remove tiny-invariant` ✅
- `mg dev --host 127.0.0.1 --port 4334` ✅
- frontend `GET / -> 200` ✅

## New Read After Extension Pass

The Node backend family is in a better place than the previous snapshot:

- `fastify` standalone ✅
- `fastify` split ✅
- `fastify` monorepo pairing ✅
- `hono` standalone ✅
- `hono` split ✅
- `trpc` standalone ✅
- `trpc` split ✅

The main Node backend family still not fully closed:

- `nestjs` still needs its own dedicated repair pass

---

## NestJS Repair Pass

This pass closed the remaining NestJS starter/runtime breakage across standalone, split fullstack, and monorepo.

### What was fixed

1. `mg dev` port wiring for split/monorepo:
   - frontend now keeps the requested `--port`
   - backend uses a stable dedicated backend port instead of sharing the frontend lane
   - this avoids frontend/backend collisions when Vite rebinds or when the requested frontend port is occupied

2. NestJS starter health route:
   - the starter no longer depends on constructor injection for the initial health endpoint
   - this avoids `tsx watch` + decorator metadata instability in the generated dev flow

3. NestJS template regeneration:
   - backend template sources regenerated
   - fullstack split template sources regenerated
   - alias split templates (`react-nestjs`, `vue-nestjs`) aligned with the same safe health pattern

### Verified workflows

#### H. Standalone backend: `nestjs --ts`

Project:

- `/private/tmp/mg-web-nest-fresh-39212/nest-fresh`

Result:

- `mg create nestjs nest-fresh --ts` ✅
- `mg install` ✅
- `mg dev --host 127.0.0.1 --port 4350` ✅
- `GET /health -> 200` ✅

Observed response:

- `{"status":"ok"}`

#### I. Split fullstack: `react-nestjs --ts`

Project:

- `/private/tmp/mg-web-nest-fresh-39212/react-nest-fresh`

Result:

- `mg create react-nestjs react-nest-fresh --ts` ✅
- `mg install` ✅
- `mg dev --host 127.0.0.1 --port 4360` ✅
- frontend `GET / -> 200` ✅
- backend `GET /api/health -> 200` ✅

Observed response:

- `{"status":"ok"}`

#### J. Monorepo: `react --monorepo --nestjs --ts`

Project:

- `/private/tmp/mg-web-nest-fresh-39212/mono-nest-fresh`

Result:

- `mg create react mono-nest-fresh --monorepo --nestjs --ts` ✅
- `mg install` ✅
- `mg dev --host 127.0.0.1 --port 4370` ✅
- frontend `GET / -> 200` ✅
- backend `GET /health -> 200` ✅

Observed response:

- `{"status":"ok"}`

### Regression status after the repair

- `cargo test -p mg --no-default-features --features web --bin mg -- --nocapture` ✅
- Result: `43 passed; 0 failed`

### Remaining reality after this pass

NestJS is no longer the blocking Node backend family for the strict local workflow lane.

Still not fully product-closed yet:

1. command-surface parity is still incomplete versus the larger design aspiration (`mg create-web`, `mg add-web`, etc. are not the active binary surface today)
2. cross-platform runtime verification is still missing for Windows/Linux
3. broader advertised framework matrix still needs the same create/install/add/remove/dev proof on every family before claiming full product readiness

---

## Command Surface Compatibility Pass

To reduce friction between single-core and future all-core installs, the web-only CLI now accepts both:

- bare commands:
  - `mg create`
  - `mg add`
  - `mg install`
  - `mg remove`
  - `mg update`
  - `mg list`
  - `mg dev`
- compatibility aliases:
  - `mg create-web`
  - `mg add-web`
  - `mg install-web`
  - `mg remove-web`
  - `mg update-web`
  - `mg list-web`
  - `mg dev-web`

This does not complete the larger all-core / single-core command-surface design yet, but it removes a current UX cliff in the web-only binary and keeps scripts closer to the intended long-term naming model.

Verification:

- `cargo test -p mg --no-default-features --features web --bin mg -- --nocapture` ✅
- `./target/debug/mg create-web --help` ✅
- `./target/debug/mg add-web --help` ✅
- `./target/debug/mg install-web --help` ✅
- `./target/debug/mg dev-web --help` ✅
