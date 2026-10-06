# mgc-lockfile

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Owns lockfile schemas and transformations for reproducible graphs: canonical parsing/serialization, v3/v4 paths, migration, ecosystem tags, root pins, merge/import/export, atomic writes, and signature/policy verification. A parseable lock does not prove signer trust was evaluated.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`canonical.rs`](../src/canonical.rs)
- [`v4.rs`](../src/v4.rs)
- [`verifier.rs`](../src/verifier.rs)
- [`writer.rs`](../src/writer.rs)

## Handoff and review

Review schema, canonical bytes, digest/signature, signer trust, owner/core tags, and atomic replacement across producers/consumers. Preserve absent/malformed/lossy/unsigned/invalid distinctions.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-lockfile`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
