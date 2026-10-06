# mgc-pack

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Builds package tarballs with file selection/ignore rules, manifest sanitization, streaming-oriented archive creation, and content hashes. Registry selection, authentication, and publication authorization belong to publish layers.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`ignore.rs`](../src/ignore.rs)
- [`manifest.rs`](../src/manifest.rs)
- [`tarball.rs`](../src/tarball.rs)

## Handoff and review

Audit inclusion/exclusion, path handling, reproducibility, and hash calculation. Digest proves byte identity, not package trust.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-pack`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
