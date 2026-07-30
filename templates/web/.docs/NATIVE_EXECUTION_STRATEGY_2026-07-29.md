# Native Execution Strategy (2026-07-29)

## Goal

MegaGate core-web should not stop at "faster install for JS projects".

It needs an execution model that can grow from:

- JS/TS ecosystem compatibility today
- to native-first execution tomorrow
- and eventually to multi-language project execution across web frontend, backend, monorepo, and fullstack shapes

This strategy borrows ideas from Bun and Vercel Labs, but does not copy their product boundaries.

## Confirmed external ideas we are borrowing

### Bun

- strict isolated install/store semantics
- shared/global cache for repeated local work
- executable build lane for JS/TS apps
- move parse/transpile overhead from runtime to build-time when possible

Official references:

- [Bun single-file executable](https://bun.sh/docs/bundler/executables)
- [Bun isolated installs](https://bun.sh/docs/pm/isolated-installs)
- [Bun runtime transpiler cache](https://bun.sh/docs/runtime/environment-variables)

### Vercel / Vercel Labs

- artifact-oriented build cache
- monorepo task graph thinking
- native executable direction for TS under a stricter compiler model

Official references:

- [vercel-labs/scriptc](https://github.com/vercel-labs/scriptc)
- [Vercel Remote Caching](https://vercel.com/docs/monorepos/remote-caching)
- [Experimental native binaries for Vercel CLI](https://vercel.com/changelog/experimental-native-binaries-for-vercel-cli)

## MegaGate interpretation

MegaGate should separate **dependency management** from **execution strategy**.

That means:

1. install/link/store layer
2. compile/build lane
3. runtime/execution lane
4. cache/artifact/security policy

If we do not separate these, "TS-to-native" becomes a marketing phrase instead of an implementable system.

## Required execution lanes

### 1. Compatibility Shell

Purpose:

- keep ecosystem compatibility with modern frontend/backend frameworks
- support React/Vite, Next.js, Vue, SvelteKit, Nuxt, Remix, and similar frameworks
- keep normal JS/TS developer ergonomics while MegaGate owns install/cache/security

Properties:

- output remains JS/TS ecosystem compatible
- `node_modules` or compatible materialization may still exist
- best default lane for scaffolded projects today

### 2. Native-Ready

Purpose:

- scaffold projects with a Rust-first bridge already present
- keep current framework working
- prepare hot paths for migration to native components

Properties:

- project contains native bridge scaffolding
- execution stays compatible by default
- useful for UI-heavy or logic-heavy projects that will need more speed later

### 3. Compiled Executable

Purpose:

- ship CLI/server/fullstack artifacts without requiring a full JS runtime installation at deploy time
- reduce startup overhead and deployment surface

Properties:

- build output becomes a self-contained executable or tightly packed artifact
- ideal for:
  - CLI tools
  - local utilities
  - backend services
  - fullstack app packaging

### 4. Native-Strict

Purpose:

- only for code paths that are sufficiently static and safe to lower to native execution
- long-term lane inspired by `scriptc`, but broader than TS-only

Properties:

- explicit constraints
- explicit diagnostics
- never silently falls back in ways that hide correctness risk

## Multi-language rule

MegaGate must not pretend one compiler can honestly replace every language backend.

Instead:

- TS/JS gets its own compiler/executable/backend strategy
- Rust stays native
- Go stays native
- Python keeps interpreter lane plus optimized packaging lane
- Java keeps JVM/native-image lane depending on project
- PHP keeps runtime lane plus deployment packaging lane

MegaGate's job is to provide:

- one policy layer
- one cache layer
- one artifact model
- one CLI experience

not to erase the strengths of language-native toolchains.

## Current project contract

Starting this pass, scaffolded `mg.toml` for web projects should carry execution metadata:

- `execution.architecture`
- `execution.lane`
- `execution.compatibility_layer`
- `execution.native_targets`

This is the first contract that lets:

- `mg create-web`
- `mg dev`
- `mg start`
- future `mg build`
- future `mg compile`

reason about the same execution story.

## What "more than TS-to-native" means for MegaGate

It means the target is:

- TS/JS executable output when valid
- Rust bridge for performance-critical logic
- shared artifact and cache system across languages
- deployment artifacts that are smaller and safer
- framework compatibility without surrendering control to npm/pnpm/bun wrappers

## Immediate next engineering steps

- keep `compatibility-shell` as the default scaffold lane
- make `native-ready` visible in scaffold/config/runtime metadata
- add execution-aware command behavior in `mg dev` / `mg start`
- prepare future commands:
  - `mg build-web --execution compiled-executable`
  - `mg compile-web`
  - `mg inspect-execution`
- continue reducing cold-path install cost, because a native story still fails if base package operations remain weak

## Non-goals for this pass

- claiming all TS/JS can already compile to native
- replacing every runtime in every language today
- removing compatibility mode before native lanes are proven

## Success criteria

This direction is healthy only if MegaGate becomes:

- faster
- lighter
- safer
- more explicit
- more multi-language capable

without losing framework compatibility during the transition.
