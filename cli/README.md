# Webapp Core (TypeScript Package Manager)

This is the MegaGate TypeScript Package Manager - a full-featured, standalone package manager with CLI.

## Structure

- **src/** - TypeScript source code
  - `cli.ts` - Main CLI entry point (install, add, update, remove, list, verify, store)
  - `installer.ts` - Installation orchestration
  - `resolver.ts` / `resolver/` - Dependency resolution & conflict handling
  - `fetcher.ts` / `fetcher/` - HTTP fetching, registry client, streaming extraction
  - `linker.ts` - Linking strategies (hardlink, symlink, copy)
  - `store.ts` / `store/` - Content-addressable store backend
  - `lock.ts` - Lockfile management
  - `security/` - Security checks (typosquat, slopsquat, minimum age, approve builds, lockdown, provenance, SBOM)
  - `config/` - Configuration management
  - `types.ts` - Shared types
  - `server.ts` - HTTP server (optional)
  - `dev.ts` - Development server
  - `build.ts` - Build utilities

- **tests/** - Unit and integration tests (vitest)
- **web/** - Static web assets
- **dist/** - Compiled JavaScript output

## Usage

```bash
cd webapp-core
pnpm install
pnpm run build
pnpm run test

# CLI
pnpm exec megagate-pm install
pnpm exec megagate-pm add <package>
pnpm exec megagate-pm list
```

## Migration

This package manager will be refactored to consume the Rust core (in `crates/`) via NAPI-RS bindings instead of its own implementation.
