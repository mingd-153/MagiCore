# Library Adapter (mgc-lib-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Routes TypeScript, Rust, Python, Go, Java/Kotlin, and .NET projects by manifest. Detection is broader than native ownership: Python accepts a supported pyproject subset, Maven differs from Gradle, and .NET requires a project file. Other cores also embed selected native helpers.

## Source map

- [`adapter.rs`](../src/adapter.rs)
- [`language.rs`](../src/language.rs)
- [`manifest.rs`](../src/manifest.rs)
- [`native/engine.rs`](../src/native/engine.rs)

## Handoff and review

Use `dependency_manifest_format` and per-project gates; language detection is insufficient. For embedded use, inspect caller overrides, core ownership, and lockfile tags.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-lib-adapter`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
