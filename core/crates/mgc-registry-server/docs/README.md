# mgc-registry-server

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Implements the separate `mgc-registry` private server and npm/PyPI/OCI, auth, storage, rate-limit, and trusted-publishing/provenance paths. Runtime config controls listener, limits, backends, OIDC issuer/audience, and trusted proxies; trace config/middleware through protocol handlers before claiming a guarantee.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`auth.rs`](../src/auth.rs)
- [`npm.rs`](../src/npm.rs)
- [`storage.rs`](../src/storage.rs)
- [`trusted.rs`](../src/trusted.rs)

## Handoff and review

Review issuer/audience/subject, admin-token requirements, proxy trust, package scope/replay, signing-key continuity, and storage assumptions. Route registration alone is not proof of correct authorization.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-registry-server`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
