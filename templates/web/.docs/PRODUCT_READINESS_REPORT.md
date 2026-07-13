# Web Product Readiness Report

Date: 2026-07-13

## Executive Summary

This pass moved the web core closer to product quality, but it is **not yet ready
to claim full production-grade framework coverage**.

What is now true:

- scaffold coverage is much stronger
- the shared version model is less error-prone
- `mg dev` supports more real frontend script shapes
- representative frontend/backend/fullstack/monorepo flows are verified

What is still not true:

- the resolver does **not** fully support npm alias dependency specs
- `mg dev` is still web-JS-first and not yet a universal runner for Go/Python/Rust/Java/PHP
- full runtime coverage for every template is not complete on this machine

## What Was Fixed In This Pass

### 1. Offline/create baseline coverage expanded

File:
- `cli/src/commands/core/web.rs`

Added curated baseline versions for the framework seed packages that previously
forced `create-web` to hit the registry:

- `nuxt`
- Angular seed packages
- Qwik seed packages
- `astro`
- `express`
- `hono`
- Nest seed packages
- `@trpc/server`
- shared seed packages such as `@types/express`, `rxjs`, `zone.js`, `zod`, `reflect-metadata`

Effect:
- scaffold can now fall back cleanly for many more frameworks

### 2. `mg dev` supports more frontend runners

File:
- `cli/src/commands/core/web.rs`

Added/expanded support for:

- `nuxt dev`
- `astro dev`
- `ng serve`
- fallback from missing `dev` script to `start`

Effect:
- Angular/Nuxt/Astro-style templates are no longer rejected immediately by the dev launcher

### 3. Shared semver handling improved

Files:
- `core/crates/mg-types/src/version.rs`
- `core/crates/mg-types/src/package.rs`

Fixes:
- stable release ordering now beats prerelease correctly
- prerelease numeric segments compare numerically
- compound ranges like `>=0.2.0 <0.5.0` now match correctly
- spaced comparator forms like `>= 2.1.2 < 3.0.0` now match correctly

Effect:
- transitive resolution is more realistic for modern npm graphs

### 4. Partial npm alias handling added in the web adapter

File:
- `adapters/web/src/lib.rs`

Change:
- dependency specs of the form `npm:real-package@range` are now detected
- alias names are preserved as dependency identities inside the web adapter
- target metadata is fetched from the real package
- an adapter test now covers alias parsing and target version lookup

Effect:
- MegaGate is no longer blind to alias dependencies
- this is a meaningful step toward Bun/pnpm-grade npm graph compatibility
- the path is still incomplete until a full real-world alias-heavy install such as Nuxt is verified end-to-end

## Matrix Results

### A. Full scaffold matrix

I reran a broad scaffold matrix after the baseline expansion.

Verified create success:

- standalone frontend: `10/10`
- standalone backend: `17/17`
- fullstack templates: `17/17`
- monorepo frontend leaf coverage: `10/10`
- monorepo backend leaf coverage: `17/17`

Total scaffold create passes:

`71/71`

That is the strongest improvement from this pass.

### B. Rust/unit test status

Verified:

- `cargo test -p mg-types`: pass `6/6`
- `cargo test -p mg-resolver`: pass `15/15`
- `cargo test -p mg`: pass `28/28`

### C. Runtime paths verified directly

Previously verified in this repo state:

- React Vite frontend: `create/install/dev` ✅
- Vue Vite frontend: `create/install/dev` ✅
- Next.js frontend: `create/install/dev` ✅
- SvelteKit frontend: `create/install/dev` ✅
- Fastify backend: `create/install/dev` ✅
- React + Fastify split fullstack: `create/install/dev` ✅
- React + Fastify monorepo: `create/install/dev` ✅

Verified in this pass:

- Vanilla frontend: `create/install/dev` ✅
  - returned `HTTP/1.1 200 OK`
- Express backend: `create/install/dev` ✅
  - `/health` returned JSON:
    - `{"status":"ok","service":"app",...}`

### D. Runtime blockers found in this pass

#### Nuxt remains unproven after alias hardening

Observed across this pass:

- before alias work, Nuxt failed with hard resolver errors around alias dependencies
- after alias work, those exact early failures were removed from the known path
- however, I do not yet have a clean `Nuxt create -> install -> dev -> 200 OK` proof in this repo state

Meaning:

- alias support is improved, but not yet fully signed off
- Nuxt is still the main product-readiness canary for modern npm compatibility

## What This Means For Product Readiness

### Stronger now

- template inventory is materially healthier
- scaffold reliability is much better
- shared resolver correctness improved
- core web can now stand up a wider set of frontend dev commands

### Still blocking product-grade claim

1. **npm alias resolution is still not fully signed off**
   - support was added at the adapter level
   - Nuxt still needs a clean end-to-end green run before this can be treated as done

2. **runtime verification is still partial**
   - scaffold coverage is broad
   - runtime coverage is representative, but not exhaustive yet

3. **non-JS backend execution is not yet a first-class `mg dev` experience**
   - on this machine I can test Go/Python/Rust with native toolchains if needed
   - but the CLI itself is not yet a universal runner for those templates

4. **Java/PHP runtime verification is not possible on this machine right now**
   - Java runtime not installed
   - Maven not installed
   - PHP not installed
   - Composer not installed

## Current Verdict

### Honest verdict

The web core is now:

- **good enough for serious continued hardening**
- **not yet ready for a blanket product launch claim**
- **not yet ready to claim parity or superiority over Bun/pnpm on resolver compatibility**

### Why

Because Bun/pnpm-grade credibility depends on correctly handling modern npm
graphs, and npm alias support is a core part of that story.

## Highest-Value Next Steps

1. finish npm alias compatibility
   - complete a clean Nuxt install/dev proof
   - verify alias materialization behavior in `node_modules`
   - add one end-to-end alias-heavy integration test

2. rerun runtime checks for:
   - `nuxt`
   - `angular`
   - `astro`
   - `qwik`
   - `solidjs`
   - node backends `hono`, `nestjs`, `trpc`

3. decide whether `mg dev` should become multi-language
   - Go
   - Python
   - Rust
   - Java
   - PHP

4. once alias support lands, rerun the runtime matrix and update `STATUS.md`

## Bottom Line

This was a meaningful product-hardening pass.

The biggest win:
- scaffold coverage is now broad and concrete

The biggest remaining blocker:
- full sign-off on npm alias-heavy installs

Until that is fixed, the web core is improved, but not yet at the level where
I would recommend calling it production-complete.
