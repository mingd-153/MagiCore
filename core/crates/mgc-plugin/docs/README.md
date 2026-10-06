# mgc-plugin

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Defines resolver, fetcher, linker, template-generator, and cache-backend plugin contracts plus a bridge around existing `PackageAdapter`s. The bridge supports gradual migration; trait availability alone does not prove production registration or use.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`resolver.rs`](../src/resolver.rs)
- [`fetcher.rs`](../src/fetcher.rs)
- [`linker.rs`](../src/linker.rs)
- [`template.rs`](../src/template.rs)

## Handoff and review

Review API/object-safety compatibility. Trace registry creation and production callers before claiming a plugin is active.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-plugin`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
