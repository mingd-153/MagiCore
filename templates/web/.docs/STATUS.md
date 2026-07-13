# Web Templates Status

Last updated: 2026-07-13

## Legend
- ✅ = complete (template.toml + all sources + FRAMEWORK_SEEDS + monorepo + tested)
- 🔶 = partial
- ❌ = missing

---

## Frontend Frameworks (10/10)

| Framework | Status | Seed | template.toml | Sources | Monorepo | Notes |
|-----------|--------|------|---------------|---------|----------|-------|
| vanilla | ✅ | ✅ | ✅ | ✅ | ✅ | |
| react-vite | ✅ | ✅ | ✅ | ✅ | ✅ | |
| vue-vite | ✅ | ✅ | ✅ | ✅ | ✅ | |
| nextjs | ✅ | ✅ | ✅ | ✅ | ✅ | |
| sveltekit | ✅ | ✅ | ✅ | ✅ | ✅ | |
| nuxt | ✅ | ✅ | ✅ | ✅ | ✅ | |
| angular | ✅ | ✅ | ✅ | ✅ | ✅ | |
| solidjs | ✅ | ✅ | ✅ | ✅ | ✅ | |
| qwik | ✅ | ✅ | ✅ | ✅ | ✅ | Dev runtime cleaned by using `vite --mode ssr` and a Qwik-compatible Vite pin |
| astro | ✅ | ✅ | ✅ | ✅ | ✅ | |

---

## Backend Frameworks — Node (5/5)

| Framework | Base | Status | template.toml | Sources | Monorepo | Notes |
|-----------|------|--------|---------------|---------|----------|-------|
| express | node | ✅ | ✅ | ✅ | ✅ | |
| fastify | node | ✅ | ✅ | ✅ | ✅ | |
| hono | node | ✅ | ✅ | ✅ | ✅ | |
| nestjs | node | ✅ | ✅ | ✅ | ✅ | |
| trpc | node | ✅ | ✅ | ✅ | ✅ | |

## Backend Frameworks — Go (3/3)

| Framework | Base | Status | template.toml | Sources | Monorepo | Notes |
|-----------|------|--------|---------------|---------|----------|-------|
| gin | go | ✅ | ✅ | ✅ | ✅ | |
| echo | go | ✅ | ✅ | ✅ | ✅ | |
| fiber | go | ✅ | ✅ | ✅ | ✅ | |

## Backend Frameworks — Python (3/3)

| Framework | Base | Status | template.toml | Sources | Monorepo | Notes |
|-----------|------|--------|---------------|---------|----------|-------|
| fastapi | python | ✅ | ✅ | ✅ | ✅ | |
| django | python | ✅ | ✅ | ✅ | ✅ | |
| flask | python | ✅ | ✅ | ✅ | ✅ | |

## Backend Frameworks — Rust (2/2)

| Framework | Base | Status | template.toml | Sources | Monorepo | Notes |
|-----------|------|--------|---------------|---------|----------|-------|
| axum | rust | ✅ | ✅ | ✅ | ✅ | |
| actix-web | rust | ✅ | ✅ | ✅ | ✅ | |

## Backend Frameworks — Java (2/2)

| Framework | Base | Status | template.toml | Sources | Monorepo | Notes |
|-----------|------|--------|---------------|---------|----------|-------|
| spring-boot | java | ✅ | ✅ | ✅ | ✅ | Native Maven runtime verified locally on 2026-07-13 |
| quarkus | java | ✅ | ✅ | ✅ | ✅ | Native Maven runtime verified locally on 2026-07-13; `mg dev` now disables Quarkus analytics prompt |

## Backend Frameworks — PHP (2/2)

| Framework | Base | Status | template.toml | Sources | Monorepo | Notes |
|-----------|------|--------|---------------|---------|----------|-------|
| laravel | php | ✅ | ✅ | ✅ | ✅ | Native Composer runtime verified locally after scaffold hardening |
| symfony | php | ✅ | ✅ | ✅ | ✅ | Native Composer runtime verified locally on 2026-07-13 after skeleton hardening (`Kernel`, framework config, `symfony/string`) |

---

## Fullstack — All-in-One (1/1)

| Framework | Status | template.toml | Sources | Notes |
|-----------|--------|---------------|---------|-------|
| remix | ✅ | ✅ | ✅ | Full Remix v2 + Vite template |

## Fullstack — Split (12/12)

| Framework | Status | template.toml | Sources | Notes |
|-----------|--------|---------------|---------|-------|
| react-fastify | ✅ | ✅ | ✅ | |
| react-express | ✅ | ✅ | ✅ | |
| react-hono | ✅ | ✅ | ✅ | |
| react-nestjs | ✅ | ✅ | ✅ | |
| react-trpc | ✅ | ✅ | ✅ | |
| react-spring | ✅ | ✅ | ✅ | React + Spring Boot (Java) |
| vue-express | ✅ | ✅ | ✅ | |
| vue-hono | ✅ | ✅ | ✅ | |
| vue-nestjs | ✅ | ✅ | ✅ | |
| vue-laravel | ✅ | ✅ | ✅ | Vue + Laravel (PHP) |
| svelte-express | ✅ | ✅ | ✅ | |
| svelte-hono | ✅ | ✅ | ✅ | |
| custom | ✅ | ✅ | ✅ | Generic fullstack placeholder |

---

## Monorepo Backend (17 frameworks across 6 languages)

| Language | Frameworks |
|----------|-----------|
| node | express, fastify, hono, nestjs, trpc |
| go | gin, echo, fiber |
| python | fastapi, django, flask |
| rust | axum, actix-web |
| java | spring-boot, quarkus |
| php | laravel, symfony |

## Monorepo Frontend (10 frameworks)

vanilla, react-vite, vue-vite, nextjs, sveltekit, nuxt, angular, solidjs, qwik, astro

---

## Shared Partials

| Layer | Status | Notes |
|-------|--------|-------|
| base/ | ✅ | |
| frontend-common/ | ✅ | |
| frontend-foundation/ | ✅ | |
| frontend-rust-ready/ | ✅ | |
| frontend/ | ✅ | |
| backend/ | ✅ | Added `exclude_features` for non-Node languages |
| fullstack/ | ✅ | |
| monorepo/ | ✅ | |
| monorepo-frontend-common/ | ✅ | |
| monorepo-frontend-foundation/ | ✅ | |
| monorepo-frontend-rust-ready/ | ✅ | |
| monorepo-frontend/ | ✅ | |
| monorepo-backend/ | ✅ | |
| monorepo-packages/ | ✅ | |

---

## Key Files

| File | Description |
|------|-------------|
| `cli/src/commands/core/web.rs` | FRAMEWORK_SEEDS, normalize aliases, fullstack mappings |
| `cli/src/scaffold/processor.rs` | WEB_FRAMEWORKS, feature injection for backend language |
| `templates/web/frontend/*/` | Frontend standalone templates + monorepo |
| `templates/web/backend/{node,go,python,rust,java,php}/*/` | Backend standalone templates + monorepo |
| `templates/web/fullstack/all-in-one/*/` | All-in-one fullstack (remix) |
| `templates/web/fullstack/split/*/` | Split fullstack (FE + BE) |
| `templates/web/shared/partials/backend/template.toml` | exclude_features for non-Node |
| `templates/web/.docs/DESIGN.md` | Template architecture guide |

---

## Scaffold Test Results

```
Status note: the list below reflects recent in-repo runtime verification and should be read together with `RUNTIME_MATRIX_2026-07-13.md`.

Verified in-repo recently:
- React Vite frontend: create/install/dev ✅
- Vue Vite frontend: create/install/dev ✅
- Vanilla frontend: create/install/dev ✅
- Angular frontend: create/install/dev ✅
- SolidJS frontend: create/install/dev ✅
- Next.js frontend: create/install/dev ✅
- SvelteKit frontend: create/install/dev ✅
- Astro frontend: create/install/dev ✅
- Nuxt frontend: create/install/dev ✅
- Qwik frontend: create/install/dev ✅
- Remix all-in-one fullstack: create/install/dev ✅
- Fastify backend: create/install/dev ✅
- Express backend: create/install/dev ✅
- Hono backend: create/install/dev ✅
- NestJS backend: create/install/dev ✅
- tRPC backend: create/install/dev ✅
- Go Echo backend: create/install/dev ✅
- Go Gin backend: create/install/dev ✅
- Go Fiber backend: create/install/dev ✅
- Rust Axum backend: create/install/dev ✅
- Rust Actix backend: create/install/dev ✅
- Python FastAPI backend: create/install/dev ✅
- Python Flask backend: create/install/dev ✅
- Python Django backend: create/install/dev ✅
- Java Spring Boot backend: create/install/dev ✅
- Java Quarkus backend: create/install/dev ✅
- PHP Laravel backend: create/install/dev ✅
- PHP Symfony backend: create/install/dev ✅
- React + Express split fullstack: create/install/dev ✅
- React + Fastify split fullstack: create/install/dev ✅
- React + Fastify monorepo: create/install/dev ✅
- React + FastAPI monorepo: create/install/dev ✅
```
