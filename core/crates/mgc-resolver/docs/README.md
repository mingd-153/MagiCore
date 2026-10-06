# mgc-resolver

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Resolves dependency graphs with version sets, registry cache, patches, and PubGrub. Source protocols include crates, Go, Maven, NuGet, Pub, PyPI, Swift, React Native, and Hugging Face; npm resolve is in the web adapter. Registry HTTP uses `mgc-http` for shared bounded network policy.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`graph.rs`](../src/graph.rs)
- [`protocols/mod.rs`](../src/protocols/mod.rs)
- [`solver/pubgrub.rs`](../src/solver/pubgrub.rs)
- [`patches.rs`](../src/patches.rs)

## Handoff and review

Check version ordering, transitive metadata, error propagation, cache freshness, and lockfile tag mapping. Preserve no-silent-skip and shared HTTP boundaries.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-resolver`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
