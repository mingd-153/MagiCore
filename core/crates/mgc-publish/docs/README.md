# mgc-publish

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Provides the client publication lane to MagiCore registries, including authentication resolution and publish/retry orchestration. It does not own tarball creation, server policy, or CLI commands; trace those layers for end-to-end changes.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`auth.rs`](../src/auth.rs)

## Handoff and review

Trace token source/scope, registry target, retry/idempotency, and server acceptance. An auth helper is not proof of end-to-end OIDC/provenance.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-publish`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
