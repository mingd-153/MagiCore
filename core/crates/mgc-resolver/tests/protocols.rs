//! Fail-closed tests for the Phase 2 registry protocol seam.
//! Test fail-closed cho điểm ghép registry protocol Phase 2.
//!
//! The npm slot remains a stub that MUST never fake a successful resolution;
//! the three native engines (crates/PyPI/pub) are real and covered by their
//! own `protocols_*.rs` suites. Here we pin the stub's fail-closed contract
//! and the shared trait defaults (sha256 verify + blake3 store_ref).
//! Slot npm vẫn là stub KHÔNG BAO GIỜ được giả resolve thành công; ba engine
//! native (crates/PyPI/pub) là thật và có suite `protocols_*.rs` riêng. Ở đây
//! ta ghim hợp đồng fail-closed của stub và default trait dùng chung (verify
//! sha256 + store_ref blake3).

#![allow(clippy::unwrap_used)]

use mgc_resolver::protocols::{NpmProtocol, RegistryProtocol, ResolvedEntry};

fn assert_unsupported(err: &mgc_types::MgError, core: &str, capability: &str) {
    match err {
        mgc_types::MgError::Unsupported {
            core: actual_core,
            capability: actual_capability,
            ..
        } => {
            assert_eq!(actual_core.to_string(), core);
            assert_eq!(actual_capability.to_string(), capability);
        }
        other => panic!("expected MgError::Unsupported, got: {other:?}"),
    }
}

#[tokio::test]
async fn npm_resolve_fails_closed() {
    let err = NpmProtocol.resolve("react", "^18").await.unwrap_err();
    assert_unsupported(&err, "web", "resolve");
}

#[tokio::test]
async fn npm_download_fails_closed() {
    let entry = ResolvedEntry {
        name: "react".to_string(),
        version: "18.0.0".to_string(),
        deps: vec![],
        artifact_url: "https://registry.npmjs.org/react/-/react-18.0.0.tgz".to_string(),
        sha256: String::new(),
        extra_markers: vec![],
    };
    let err = NpmProtocol.download(&entry).await.unwrap_err();
    assert_unsupported(&err, "web", "fetch");
}

#[test]
fn default_verify_sha256_mismatch_fails_closed() {
    let entry = ResolvedEntry {
        name: "serde".to_string(),
        version: "1.0.0".to_string(),
        deps: vec![],
        artifact_url: String::new(),
        sha256: "deadbeef".to_string(),
        extra_markers: vec![],
    };
    let err = NpmProtocol.verify(&entry, b"actual bytes").unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");
}

#[test]
fn default_verify_empty_declared_sha256_is_not_a_failure() {
    let entry = ResolvedEntry {
        name: "serde".to_string(),
        version: "1.0.0".to_string(),
        deps: vec![],
        artifact_url: String::new(),
        sha256: String::new(),
        extra_markers: vec![],
    };
    NpmProtocol.verify(&entry, b"bytes").unwrap();
}

#[test]
fn default_store_ref_matches_blake3_cas_layout() {
    let bytes = b"hello native registry";
    let expected_hex = blake3::hash(bytes).to_hex().to_string();
    let expected = format!("files/blake3/{}/{}", &expected_hex[..2], expected_hex);
    assert_eq!(NpmProtocol.store_ref(bytes), expected);
}
