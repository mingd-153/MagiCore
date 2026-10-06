# mgc-types

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Defines shared contracts and domain types: adapters/capabilities, ecosystems, package/version identity, manifests, publish/patch models, errors/results, JSONC, and secret censoring. Public changes may affect most crates and require caller plus serialization/API compatibility review.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`adapter.rs`](../src/adapter.rs)
- [`capabilities.rs`](../src/capabilities.rs)
- [`package.rs`](../src/package.rs)
- [`manifest.rs`](../src/manifest.rs)

## Handoff and review

Analyze callers before changing public traits/enums or identity semantics. Check serialization, equality/order/hash, ecosystem ownership strings, and core parity.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-types`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
