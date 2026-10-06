# mgc-fetcher

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Provides lower-level shared download and archive-extraction helpers. Higher layers own registry identity, auth, trust, and install policy; this crate owns transfer/extraction mechanics.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`download.rs`](../src/download.rs)
- [`extract.rs`](../src/extract.rs)

## Handoff and review

Review caller digest verification and extraction path policy too. A successful transfer alone does not authenticate an artifact; preserve safe extraction and error handling.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-fetcher`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
