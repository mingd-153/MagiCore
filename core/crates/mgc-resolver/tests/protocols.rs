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

use async_trait::async_trait;
use mgc_resolver::protocols::{NpmProtocol, RegistryProtocol, ResolvedEntry};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

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
fn default_verify_empty_declared_sha256_fails_closed() {
    // V1.2 zero-trust (D0): an artifact no registry hash vouches for is
    // never installed — the old fail-open (`Ok(())` on empty) was a
    // supply-chain hole, now closed at the shared default.
    let entry = ResolvedEntry {
        name: "serde".to_string(),
        version: "1.0.0".to_string(),
        deps: vec![],
        artifact_url: String::new(),
        sha256: String::new(),
        extra_markers: vec![],
    };
    let err = NpmProtocol.verify(&entry, b"bytes").unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");
    assert!(
        err.to_string().contains("without digest"),
        "the error must say why: {err}"
    );
}

#[test]
fn default_store_ref_matches_blake3_cas_layout() {
    let bytes = b"hello native registry";
    let expected_hex = blake3::hash(bytes).to_hex().to_string();
    let expected = format!("files/blake3/{}/{}", &expected_hex[..2], expected_hex);
    assert_eq!(NpmProtocol.store_ref(bytes), expected);
}

struct ConflictingGraphProtocol;

#[async_trait]
impl RegistryProtocol for ConflictingGraphProtocol {
    async fn resolve(&self, name: &str, range: &str) -> mgc_types::MgResult<ResolvedEntry> {
        let (version, deps) = match name {
            "root" => (
                "1.0.0",
                vec![
                    ("left".to_string(), "*".to_string()),
                    ("right".to_string(), "*".to_string()),
                ],
            ),
            "left" => ("1.0.0", vec![("shared".to_string(), "^1".to_string())]),
            "right" => ("1.0.0", vec![("shared".to_string(), "^2".to_string())]),
            "shared" if range == "^1" => ("1.5.0", vec![]),
            "shared" if range == "^2" => ("2.1.0", vec![]),
            "shared" => ("2.1.0", vec![]),
            _ => {
                return Err(mgc_types::MgError::Other(format!(
                    "unexpected package {name}"
                )));
            }
        };
        Ok(ResolvedEntry {
            name: name.to_string(),
            version: version.to_string(),
            deps,
            artifact_url: format!("https://registry.invalid/{name}/{version}"),
            sha256: "00".repeat(32),
            extra_markers: vec![],
        })
    }

    async fn download(&self, _entry: &ResolvedEntry) -> mgc_types::MgResult<Vec<u8>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn graph_resolution_rejects_conflicting_transitive_ranges() {
    let error = ConflictingGraphProtocol
        .resolve_graph("root", "*")
        .await
        .unwrap_err();
    assert!(matches!(error, mgc_types::MgError::DependencyConflict(_)));
    assert!(
        error
            .to_string()
            .contains("incompatible constraints for shared")
    );
}

struct ConcurrentGraphProtocol {
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
}

#[async_trait]
impl RegistryProtocol for ConcurrentGraphProtocol {
    async fn resolve(&self, name: &str, _range: &str) -> mgc_types::MgResult<ResolvedEntry> {
        let current = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(current, Ordering::SeqCst);
        let delay_ms = name
            .strip_prefix("dep-")
            .and_then(|index| index.parse::<u64>().ok())
            .map(|index| 32 - index)
            .unwrap_or(1);
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        self.active.fetch_sub(1, Ordering::SeqCst);

        let deps = if name == "root" {
            (0..32)
                .map(|index| (format!("dep-{index}"), "*".to_string()))
                .collect()
        } else {
            Vec::new()
        };
        Ok(ResolvedEntry {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            deps,
            artifact_url: format!("https://registry.invalid/{name}/1.0.0"),
            sha256: "00".repeat(32),
            extra_markers: Vec::new(),
        })
    }

    async fn download(&self, _entry: &ResolvedEntry) -> mgc_types::MgResult<Vec<u8>> {
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn graph_resolution_resolves_each_bfs_layer_concurrently_and_keeps_order() {
    let protocol = ConcurrentGraphProtocol {
        active: Arc::new(AtomicUsize::new(0)),
        max_active: Arc::new(AtomicUsize::new(0)),
    };

    let entries = protocol.resolve_graph("root", "*").await.unwrap();

    assert_eq!(protocol.max_active.load(Ordering::SeqCst), 16);
    assert_eq!(entries.len(), 33);
    assert_eq!(entries[0].name, "root");
    for (index, entry) in entries.iter().skip(1).enumerate() {
        assert_eq!(entry.name, format!("dep-{index}"));
    }
}

#[tokio::test]
async fn graph_resolution_bounds_concurrent_roots_and_keeps_root_order() {
    let protocol = ConcurrentGraphProtocol {
        active: Arc::new(AtomicUsize::new(0)),
        max_active: Arc::new(AtomicUsize::new(0)),
    };
    let roots = (0..32)
        .map(|index| (format!("dep-{index}"), "*".to_string()))
        .collect::<Vec<_>>();

    let entries = protocol.resolve_graph_roots(&roots).await.unwrap();

    assert!(protocol.max_active.load(Ordering::SeqCst) > 1);
    assert!(protocol.max_active.load(Ordering::SeqCst) <= 16);
    assert_eq!(entries.len(), roots.len());
    for (entry, (root, _)) in entries.iter().zip(roots) {
        assert_eq!(entry.name, root);
    }
}
