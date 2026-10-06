# mgc-oci

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Implements OCI Distribution operations for catalog/tag listing, blob and manifest pull/push, bearer-token setup, and downloaded-blob digest validation. It supplies transport mechanics; package/model identity and higher-level authorization are caller responsibilities.

## Source map

- [`client.rs`](../src/client.rs)
- [`manifest.rs`](../src/manifest.rs)
- [`ref.rs`](../src/ref.rs)

## Handoff and review

Check reference/URL construction, auth propagation, statuses, digests, and upload behavior. OCI transport does not establish all-core trusted publishing/provenance.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-oci`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
