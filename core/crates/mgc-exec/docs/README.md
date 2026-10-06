# mgc-exec

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Centralizes controlled external-process passthrough through executable/argument allowlists, a process wrapper, audit records, and secret sanitization. It is a process-policy boundary, not a generic arbitrary-shell API.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`allowlist.rs`](../src/allowlist.rs)
- [`run.rs`](../src/run.rs)
- [`audit.rs`](../src/audit.rs)
- [`sanitizer.rs`](../src/sanitizer.rs)

## Handoff and review

Treat allowlist/sanitizer changes as security-sensitive. Preserve deny-by-default and verify arguments and audit records remain sanitized on errors.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-exec`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
