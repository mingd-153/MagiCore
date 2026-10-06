# Mobile App Adapter (mgc-app-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Detects Flutter, Kotlin, Swift, React Native, Objective-C, and multi-platform projects, then routes manifest parsing/writing and audit/cache/install helpers. Resolver capability is language-specific; detected languages are not a blanket claim of complete dependency lifecycle support.

## Source map

- [`adapter.rs`](../src/adapter.rs)
- [`language.rs`](../src/language.rs)
- [`manifest/mod.rs`](../src/manifest/mod.rs)
- [`native/mod.rs`](../src/native/mod.rs)

## Handoff and review

Trace each language through detection, manifest ownership, capability reporting, and CLI dispatch. A parser or marker alone is not evidence that install is supported.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-app-adapter`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
