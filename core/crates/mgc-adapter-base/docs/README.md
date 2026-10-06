# mgc-adapter-base

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Provides shared `BaseAdapter` defaults and guarded project-file helpers. It parses/writes Cargo manifests and supports regular-file reads and atomic project-file writes; it is adapter infrastructure, not an ecosystem resolver.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`cargo_manifest.rs`](../src/cargo_manifest.rs)
- [`project_file.rs`](../src/project_file.rs)

## Handoff and review

Defaults affect every adopter. Inspect implementors, symlink/special-file handling, atomic replacement, and error propagation before changing shared behavior.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-adapter-base`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
