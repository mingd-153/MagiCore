# mgc-crypto

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Provides BLAKE3, checksums, Ed25519 signing/verification, integrity types, keyring persistence, and SIMD detection. It supplies primitives; authorization policy for trusted keys, identities, and artifacts belongs to consuming layers.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`ed25519_signer.rs`](../src/ed25519_signer.rs)
- [`keyring.rs`](../src/keyring.rs)
- [`integrity.rs`](../src/integrity.rs)
- [`blake3_signer.rs`](../src/blake3_signer.rs)

## Handoff and review

Preserve format checks, key-file permissions, and integrity mismatch errors. Keep cryptographic validity separate from trust authorization.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-crypto`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
