//! PyPI index output safety tests.
//! Kiểm tra an toàn đầu ra index PyPI.

use super::*;
use axum::{Router, body::to_bytes, http::Request};
use tower::ServiceExt;

#[tokio::test]
async fn simple_index_escapes_untrusted_file_metadata() {
    let temp = tempfile::tempdir().expect("temporary registry directory");
    let store = std::sync::Arc::new(
        crate::storage::RegistryStore::new(temp.path())
            .await
            .expect("registry store"),
    );
    store
        .put_pypi_file(&PypiFile {
            name: "demo".into(),
            version: "1.0.0".into(),
            filename: "demo-1.0.0\"><script>alert(1)</script>.whl".into(),
            digest: format!("sha256:{}", "ab".repeat(32)),
            size: 0,
            requires_python: Some("\" onmouseover=\"alert(1)".into()),
        })
        .await
        .expect("store PyPI file metadata");
    let auth = std::sync::Arc::new(crate::auth::AuthService::new(None, store.clone()));
    let app: Router = routes().with_state((store, auth));
    let response = app
        .oneshot(
            Request::get("/pypi/simple/demo")
                .body(axum::body::Body::empty())
                .expect("simple-index request"),
        )
        .await
        .expect("simple-index response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 16 * 1024)
        .await
        .expect("read simple-index response");
    let html = String::from_utf8(body.to_vec()).expect("UTF-8 HTML");
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("%22%3E%3Cscript%3Ealert%281%29%3C%2Fscript%3E"));
    assert!(html.contains("&quot; onmouseover=&quot;alert(1)"));
    assert!(html.contains("href=\"/pypi/packages/demo/"));
}
