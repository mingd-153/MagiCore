# mgc-http

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Provides shared outbound HTTP operations: TLS, timeouts, retries, proxy, offline handling, rate limits, cache, telemetry, and upload. Registry/artifact clients should use this boundary so network behavior remains consistently configurable and reviewable.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`methods.rs`](../src/methods.rs)
- [`retry.rs`](../src/retry.rs)
- [`tls.rs`](../src/tls.rs)
- [`upload.rs`](../src/upload.rs)

## Handoff and review

Network changes affect every caller. Check retry/idempotency, cache/offline fallback, TLS, and secret-safe telemetry; prevent raw-HTTP bypasses.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-http`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
