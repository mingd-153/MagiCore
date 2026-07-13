# Web Templates Design

This directory defines the canonical template layout for `mg init` under the `web` core.

The goal is not just scaffolding convenience. The template tree is part of the product design for `mg/web`:

- fast project creation
- low-noise structure
- support for frontend, backend, fullstack, monorepo, and vanilla
- compatibility with many frameworks
- room for a native `mg/web` install/link/store pipeline later
- room for shared compiler and generation building blocks

`mg/web` is not a wrapper around pnpm, bun, yarn, or other package managers. These templates must stay aligned with a native MegaGate web core.

## Design Rules

1. `mg init` drives the user flow.
2. `templates/web/` is the source of truth for scaffold structure.
3. `cli/src/scaffold/processor.rs` should process templates from this tree, not hardcode file contents.
4. Template naming must match wizard values where possible.
5. Output structure must stay simple, stable, and easy to maintain.
6. Monorepo must be flexible, but it should grow carefully and not become chaotic again.
7. Shared components may be reused aggressively, but the final scaffold must still match the requested framework and core-web requirements.
8. The scaffold compiler is Rust. Generated app code follows the target framework language/runtime.
9. For single-core web create flows, JavaScript is the default output. TypeScript is opt-in via `--ts`.

## Web Modes

The web core supports four top-level project modes:

- `frontend`
- `backend`
- `fullstack`
- `monorepo`

These modes mirror `cli/src/wizard/web.rs`.

## Proposed Structure

```text
templates/web/
├── README.md
├── shared/                      # shared pieces reused by many templates
│   ├── gitignore/
│   ├── configs/
│   ├── partials/
│   ├── packages/
│   ├── compiler/
│   └── generators/
│
├── frontend/
│   ├── vanilla/
│   ├── react-vite/
│   ├── vue-vite/
│   ├── nextjs/
│   ├── sveltekit/
│   ├── nuxt/
│   ├── angular/
│   ├── solidjs/
│   ├── qwik/
│   └── astro/
│
├── backend/
│   ├── node/
│   │   ├── express/
│   │   ├── fastify/
│   │   ├── nestjs/
│   │   ├── hono/
│   │   └── trpc/
│   ├── php/
│   │   ├── laravel/
│   │   └── symfony/
│   ├── java/
│   │   ├── spring-boot/
│   │   └── quarkus/
│   ├── go/
│   │   ├── gin/
│   │   ├── echo/
│   │   └── fiber/
│   ├── python/
│   │   ├── fastapi/
│   │   ├── django/
│   │   └── flask/
│   └── rust/
│       ├── axum/
│       └── actix-web/
│
├── fullstack/
│   ├── all-in-one/
│   │   ├── nextjs/
│   │   ├── nuxt/
│   │   ├── sveltekit/
│   │   └── remix/
│   └── split/
│       ├── react-fastify/
│       ├── vue-laravel/
│       ├── react-spring/
│       └── custom/
│
└── monorepo/
    ├── base/
    ├── frontend/
    │   ├── vanilla/
    │   ├── react-vite/
    │   ├── vue-vite/
    │   ├── nextjs/
    │   ├── sveltekit/
    │   ├── nuxt/
    │   ├── angular/
    │   ├── solidjs/
    │   ├── qwik/
    │   └── astro/
    ├── backend/
    │   ├── node/
    │   │   ├── express/
    │   │   ├── fastify/
    │   │   ├── nestjs/
    │   │   ├── hono/
    │   │   └── trpc/
    │   ├── php/
    │   │   ├── laravel/
    │   │   └── symfony/
    │   ├── java/
    │   │   ├── spring-boot/
    │   │   └── quarkus/
    │   ├── go/
    │   │   ├── gin/
    │   │   ├── echo/
    │   │   └── fiber/
    │   ├── python/
    │   │   ├── fastapi/
    │   │   ├── django/
    │   │   └── flask/
    │   └── rust/
    │       ├── axum/
    │       └── actix-web/
    └── packages/
```

## Shared Template Lanes

To avoid duplicating the same UI and branding logic across many web frameworks, the scaffold now uses layered partials:

- `shared/partials/frontend-common/`
  - shared frontend UI shell
  - shared theme
  - shared brand config
  - shared link hook
  - shared favicon and logo assets
- `shared/partials/frontend/`
  - single-frontend content copy
- `shared/partials/monorepo-frontend-common/`
  - the same shared frontend shell, but targeted to `apps/frontend/...`
- `shared/partials/monorepo-frontend/`
  - monorepo-specific content copy
- `shared/partials/backend/`
  - backend config, routes, and shared service building blocks
- `shared/partials/monorepo-backend/`
  - backend structure targeted to `apps/backend/...`

This keeps the Rust scaffold compiler in control while making later UI or branding changes much easier.

## Generated Structure Targets

The generated project should not dump everything into a single `src` bucket.

### Frontend React/Vite

```text
src/
├── assets/
├── bridges/
├── components/
├── config/
├── content/
├── hooks/
├── router/
├── styles/
├── App.tsx
└── main.tsx

crates/
└── engine/
```

### Frontend Next.js

```text
src/
├── app/
├── assets/
├── bridges/
├── components/
├── config/
├── content/
├── hooks/
└── styles/

crates/
└── engine/
```

### Monorepo

```text
apps/
├── frontend/
│   └── src/
│       ├── assets/
│       ├── bridges/
│       ├── components/
│       ├── config/
│       ├── content/
│       ├── hooks/
│       ├── router/
│       └── styles/
│
│   └── crates/
│       └── engine/
└── backend/
    └── src/
        ├── config/
        ├── lib/
        ├── routes/
        └── services/
```

## Mapping to `mg init`

The wizard currently emits these shape categories:

- `frontend -> framework`
- `backend -> language -> framework`
- `fullstack -> stack`
- `monorepo -> frontend framework + backend framework`

So template resolution should follow these rules:

### Frontend

```text
sub_type = "frontend"
frameworks = ["react-vite"]
=> templates/web/frontend/react-vite/
```

### Backend

```text
sub_type = "backend"
frameworks = ["node", "fastify"]
=> templates/web/backend/node/fastify/
```

### Fullstack All-in-one

```text
sub_type = "fullstack"
frameworks = ["nextjs"]
=> templates/web/fullstack/all-in-one/nextjs/
```

### Fullstack Split

```text
sub_type = "fullstack"
frameworks = ["react-fastify"]
=> templates/web/fullstack/split/react-fastify/
```

### Monorepo

```text
sub_type = "monorepo"
frameworks = ["react-vite", "fastify"]
=> templates/web/monorepo/base/
 + templates/web/monorepo/frontend/react-vite/
 + templates/web/monorepo/backend/node/fastify/
 + optional packages/features from shared building blocks
```

## Current Folder Reality

Current folders in this repo:

```text
templates/web/
├── next/
├── react-vite/
├── next-app/
├── react/
├── vanilla/
└── vue-vite/
```

This is not yet aligned with the wizard or with the long-term web-core design.

## Problems to Fix

### 1. Flat structure

The current folder tree is mostly flat. It does not distinguish:

- frontend
- backend
- fullstack
- monorepo

That makes `mg init` mapping ambiguous.

### 2. Naming mismatch

The wizard uses names like:

- `nextjs`
- `sveltekit`

But current template folders use:

- `next/`
- `next-app/`
- `svelte/`

This will create translation bugs unless naming is normalized.

### 3. Missing backend templates

The wizard already offers backend frameworks, but `templates/web/` does not yet represent them.

### 4. Monorepo should not be a few hardcoded combinations

Monorepo should be composed from separate frontend and backend selections. It should not be limited to a few preset pairings.

### 5. Missing reusable building blocks

The design needs shared pieces for:

- common config
- common package layout
- shared contracts
- compiler or codegen steps
- generation helpers

### 6. Hardcoded scaffolding in CLI

Current CLI scaffolding writes files from Rust code. That is temporary and should be removed in favor of template processing from this directory.

## Naming Standard

## Compiler vs Output

Two layers must stay separate:

1. MegaGate scaffold compiler:
   - implemented in Rust
   - validates template contracts
   - selects files/layers/features
   - should remain the source of truth

2. Generated project code:
   - should match the target framework ecosystem
   - React/Vite or Next.js can emit JS by default
   - `--ts` upgrades the scaffold to TypeScript files and TypeScript toolchain dependencies

This means:

- do not replace the Rust scaffold compiler with a TypeScript-based generator
- do generate TS when the user explicitly requests `--ts`
- do keep the default single-core flow simple:

```text
mg create react@latest myApp
=> JavaScript scaffold

mg create react@latest myApp --ts
=> TypeScript scaffold
```

Use wizard output values as the canonical template IDs:

- `nextjs`
- `react-vite`
- `vue-vite`
- `sveltekit`
- `vanilla`
- `express`
- `fastify`
- `nestjs`
- `laravel`
- `spring-boot`
- `fastapi`
- `axum`

If product naming needs a nicer label, keep that in the wizard label only. Folder names should stay stable and machine-oriented.

## Template Compiler Contract

`mg/web` should use a Rust template compiler, not a generic text-template engine.

The compiler contract for each template layer is:

- `template.toml` defines the layer manifest
- `sources/` stores source files
- Rust validates:
  - manifest shape
  - source existence
  - output target collisions inside a layer
  - declared context keys
  - actual token usage in source files
- materialization fails early if the contract is broken

Suggested files per template:

```text
template.toml           # typed layer manifest consumed by Rust
sources/                # source files compiled by Rust into output files
partials/               # optional local sub-layers later
hooks/                  # optional generation hooks later
```

Suggested files for shared building blocks:

```text
templates/web/shared/
├── configs/
├── partials/
├── packages/
├── compiler/
└── generators/
```

These can be used for:

- common workspace files
- shared `apps/` or `packages/` skeletons
- code generation
- rust-to-js or rust-to-ts compiler-assisted output later if the core-web architecture needs it
- framework-specific assembly from reusable pieces

Example:

```text
templates/web/frontend/react-vite/
├── template.toml
└── sources/
    ├── package.json
    ├── index.html
    └── src/main.tsx
```

Monorepo example:

```text
templates/web/monorepo/base/
├── template.toml
└── sources/
    └── megagate.workspace.toml
```

## Vanilla Support

`vanilla` is a first-class path, not a fallback.

It should stay supported because it is:

- the lightest frontend option
- the best baseline for performance testing
- useful for debugging the native `mg/web` install/link/runtime behavior

## Backend and Framework Breadth

The web core is expected to support many frameworks, including backend stacks, because the web domain is broader than frontend package installation.

That means `templates/web/` must be designed for:

- frontend-only apps
- backend-only web services
- fullstack web apps
- monorepos with split FE/BE services

## Monorepo Direction

The canonical monorepo output should currently lean toward:

```text
apps/frontend/
apps/backend/
packages/
```

This matches existing project experience and keeps the structure understandable.

`packages/` is intentionally flexible. It may later include shared contracts, shared config, UI packages, utilities, or compiler-generated packages, but monorepo should evolve carefully rather than becoming overbuilt too early.

## Feature-Guided Shared Schema

Shared schema or contract layers such as `zod` should not be hardcoded into every project.

They should be:

- suggested intelligently by `mg init`
- enabled when the selected project shape benefits from them
- especially useful for monorepo or split fullstack projects

That means schema/contracts are a feature-layer decision, not a mandatory baseline.

## Recommended Next Direction

Before more web scaffolding code is written, the project should:

1. normalize template IDs to match the wizard
2. split `templates/web/` into mode-based directories
3. define reusable shared building blocks under `templates/web/shared/`
4. define template metadata format
5. update `cli/src/scaffold/processor.rs` to resolve templates from this tree
6. keep `_archive/web-pm-v1/` as technical reference only

## Canonical Principle

For `core-web`, `mg init` should ask the user what they want to build, then map deterministically into one template path under `templates/web/`.

The template tree must therefore be:

- explicit
- mode-aware
- framework-aware
- backend/frontend aware
- shared-component aware
- maintainable
- ready for a native MegaGate web core
