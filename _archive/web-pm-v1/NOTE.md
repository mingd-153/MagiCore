# Archived Web PM v1

This is the original MegaGate web package manager implementation (40k+ lines Rust).

**Status**: Archived for reference and code extraction.  
**Tests**: 811/811 passing  
**Date Archived**: 2026-07-07

## Purpose
Keep this code for:
- Reference during refactor
- Extracting reusable components to `core/`
- Ensuring no functionality is lost

## DO NOT
- Delete this folder
- Modify code here (read-only)
- Use in production (use new structure instead)

## Code Extraction Map
- `crates/mg-store/` → `core/crates/mg-store/`
- `crates/mg-resolver/` → `core/crates/mg-resolver/`
- `crates/mg-lockfile/` → `core/crates/mg-lockfile/`
- `crates/mg-registry/` → `adapters/web/src/npm_registry.rs`
- `crates/mg-fetcher/` → `core/crates/mg-fetcher/`
- `crates/mg-core/src/cffi/sha256.rs` → `core/crates/mg-crypto/`
