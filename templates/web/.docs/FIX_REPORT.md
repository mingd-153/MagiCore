# Web Fix Report

Date: 2026-07-13

## Scope

This pass fixed the verified breakages found during the recent web audit:

- stale test expectations in CLI scaffold tests
- incorrect prerelease version ordering in the shared version model
- unstable Next.js TypeScript/runtime defaults
- outdated status reporting in `STATUS.md`

## What Changed

### 1. Fixed semver prerelease ordering at the root

File:
- `core/crates/mg-types/src/version.rs`

Change:
- stable releases now sort after prereleases for the same `major.minor.patch`
- prerelease numeric identifiers now compare numerically instead of lexicographically

Why it mattered:
- the old comparator could pick `1.0.0-next.9` over `1.0.0-next.24`
- that broke transitive dependency selection for real packages such as `@polka/url`

Added checks:
- stable > prerelease
- `next.24` > `next.9`

### 2. Fixed web scaffold/unit tests

Files:
- `cli/src/scaffold/processor.rs`
- `cli/src/commands/core/web.rs`

Change:
- removed stale assertions that still expected some frameworks to be unimplemented
- updated TypeScript baseline assertions from `^7.0.2` to `^5.9.2`

Result:
- `cargo test -p mg` is green again

### 3. Stabilized Next.js create/dev flow

Files:
- `cli/src/commands/core/web.rs`
- `templates/web/frontend/nextjs/sources/package.json`
- `templates/web/frontend/nextjs/sources/tsconfig.json`

Change:
- `mg dev` now accepts `next dev` with extra flags
- Next template now uses `next dev --webpack`
- Next template `tsconfig.json` now includes the values Next was auto-injecting at runtime
- web scaffold baseline TypeScript was moved to the `5.9.x` lane

Why:
- the previous flow failed during `mg dev`
- Next also rewrote `tsconfig.json` on first boot

Current verified behavior:
- project creates
- `mg install-web` completes
- `mg dev` boots successfully on localhost

### 4. Revalidated SvelteKit flow after resolver fix

Files:
- no SvelteKit template file change was required in this pass

Why it started working:
- the resolver now picks a correct prerelease candidate for transitive packages

Verified installed result:
- `@polka/url` resolved to `1.0.0-next.29`

This replaced the previously broken `1.0.0-next.9` result that caused dev startup failure.

### 5. Corrected status documentation

File:
- `templates/web/.docs/STATUS.md`

Change:
- removed the misleading "treat this as runtime truth" posture
- replaced it with a re-validation note
- updated the verified list with the flows that were actually rerun

## Verified Outputs

### Rust tests

`cargo test -p mg-types`
- pass: `4/4`

`cargo test -p mg-resolver`
- pass: `15/15`

`cargo test -p mg`
- pass: `26/26`

### Runtime checks

#### Next.js

Create:
```bash
mg create-web next@latest /private/tmp/mg-next-fix.../app --ts
```

Install:
```text
28 packages installed
91400328 from cache
31682 ms total
```

Dev:
```text
Next.js 16.2.10 (webpack)
Local: http://127.0.0.1:4318
Ready in 1180ms
```

Probe:
```text
HTTP/1.1 200 OK
X-Powered-By: Next.js
```

#### SvelteKit

Create:
```bash
mg create-web svelte@latest /private/tmp/mg-svelte-fix.../app --ts
```

Install:
```text
52 packages installed
13122759 from cache
18038 ms total
```

Dev:
```text
VITE v8.1.4 ready
Local: http://127.0.0.1:4317
```

Probe:
```text
HTTP/1.1 200 OK
x-sveltekit-page: true
```

## Important Notes

### Temporary compatibility choice

Next.js currently uses:
```json
"dev": "next dev --webpack"
```

This is deliberate.

It is the smaller, safer fix right now because it restores a working `mg dev`
path without reopening the earlier Turbopack/root-resolution failure.

### What this pass did not claim

- it did not prove every framework in `templates/web/` is production-ready
- it did not benchmark MegaGate against Bun or pnpm
- it did not certify every monorepo/backend/fullstack variant after the latest template churn

## Recommended Next Pass

1. add an integration matrix for the frameworks that matter most first:
   - `react-vite`
   - `vue-vite`
   - `nextjs`
   - `sveltekit`
   - `fastify`
   - `react-fastify`
   - `monorepo react-vite + fastify`
2. add one resolver test that locks the `@polka/url` prerelease case
3. decide when to revisit Turbopack instead of `--webpack`
4. only after that, widen the matrix to the rest of the new template inventory
