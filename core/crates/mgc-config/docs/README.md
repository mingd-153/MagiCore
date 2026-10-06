# mgc-config

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Owns shared project/user config models, registry chains, npmrc credential/registry mapping, hooks, and project-core attestations. Consumers should use centralized parsers and policy objects instead of inventing local formats.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`project.rs`](../src/project.rs)
- [`chain.rs`](../src/chain.rs)
- [`npmrc.rs`](../src/npmrc.rs)
- [`attestation.rs`](../src/attestation.rs)

## Handoff and review

Config affects security and routing. Trace precedence, path ownership, secret handling, and fail-closed defaults through callers before changing parsers.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-config`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
