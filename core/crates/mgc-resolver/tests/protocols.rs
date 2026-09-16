//! Fail-closed tests for the Phase 2 registry protocol slots.
//! Test fail-closed cho các chỗ cắm registry protocol Phase 2.
//!
//! A stub must NEVER fake a successful resolution — every call has to come
//! back as `MgError::Unsupported` with the right core + capability tags so
//! callers (and the audit trail) see an honest "not implemented" verdict.
//! Stub không bao giờ được giả resolve thành công — mọi lời gọi phải trả
//! `MgError::Unsupported` với đúng nhãn core + capability để caller (và audit
//! trail) thấy một phán quyết "chưa implement" trung thực.

use mgc_resolver::protocols::{
    CratesProtocol, NpmProtocol, PubProtocol, PypiProtocol, RegistryProtocol,
};

fn assert_unsupported(err: &mgc_types::MgError, core: &str) {
    match err {
        mgc_types::MgError::Unsupported {
            core: actual_core,
            capability,
            ..
        } => {
            assert_eq!(actual_core.to_string(), core);
            assert_eq!(capability.to_string(), "resolve");
        }
        other => panic!("expected MgError::Unsupported, got: {other:?}"),
    }
}

#[tokio::test]
async fn npm_stub_fails_closed() {
    let err = NpmProtocol
        .resolve_package("react", "^18")
        .await
        .unwrap_err();
    assert_unsupported(&err, "web");
}

#[tokio::test]
async fn crates_stub_fails_closed() {
    let err = CratesProtocol
        .resolve_package("serde", "1.0")
        .await
        .unwrap_err();
    assert_unsupported(&err, "rust");
}

#[tokio::test]
async fn pypi_stub_fails_closed() {
    let err = PypiProtocol
        .resolve_package("numpy", ">=1.26")
        .await
        .unwrap_err();
    assert_unsupported(&err, "python");
}

#[tokio::test]
async fn pub_stub_fails_closed() {
    let err = PubProtocol
        .resolve_package("path", "^1.0")
        .await
        .unwrap_err();
    assert_unsupported(&err, "app");
}
