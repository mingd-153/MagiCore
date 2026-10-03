use super::*;
use axum::{Router, body::Body, http::Request};
use tower::ServiceExt;

fn package_with_tarball(tarball: &str) -> Package {
    let text = serde_json::json!({
        "name": "demo",
        "description": null,
        "versions": {
            "1.0.0": {
                "name": "demo",
                "version": "1.0.0",
                "description": null,
                "dist": {
                    "integrity": "sha512-x",
                    "shasum": "y",
                    "tarball": tarball
                }
            }
        }
    });
    serde_json::from_value(text).expect("fixture package")
}

#[test]
fn forwarded_https_is_preserved_for_proxied_deploys() {
    let mut pkg = package_with_tarball("http://old-host/demo/-/demo-1.0.0.tgz");
    rewrite_tarball_host(&mut pkg, "example.com", "https");
    assert_eq!(
        pkg.versions["1.0.0"].dist.tarball,
        "https://example.com/demo/-/demo-1.0.0.tgz"
    );
}

#[test]
fn local_default_stays_plain_http() {
    let mut pkg = package_with_tarball("http://old-host/demo/-/demo-1.0.0.tgz");
    rewrite_tarball_host(&mut pkg, "127.0.0.1:4315", "http");
    assert_eq!(
        pkg.versions["1.0.0"].dist.tarball,
        "http://127.0.0.1:4315/demo/-/demo-1.0.0.tgz"
    );
}

#[test]
fn unknown_scheme_falls_back_to_http_never_bare() {
    let mut pkg = package_with_tarball("http://old-host/demo/-/demo-1.0.0.tgz");
    rewrite_tarball_host(&mut pkg, "example.com", "gopher;rm -rf");
    assert!(pkg.versions["1.0.0"].dist.tarball.starts_with("http://"));
}

#[tokio::test]
async fn publish_rejects_one_attachment_mapped_to_multiple_versions() {
    let temp = tempfile::tempdir().expect("temporary registry directory");
    let store = std::sync::Arc::new(
        crate::storage::RegistryStore::new(temp.path())
            .await
            .expect("registry store"),
    );
    let auth = std::sync::Arc::new(crate::auth::AuthService::new(None, store.clone()));
    let app: Router = routes().with_state((store.clone(), auth));
    let body = serde_json::json!({
        "name": "demo",
        "versions": {
            "1.0.0": {"name":"demo","version":"1.0.0","dist":{"integrity":"sha512-old1","shasum":"old1","tarball":"http://old/demo-1.tgz"}},
            "2.0.0": {"name":"demo","version":"2.0.0","dist":{"integrity":"sha512-old2","shasum":"old2","tarball":"http://old/demo-2.tgz"}}
        },
        "_attachments": {
            "demo-1.0.0.tgz": {"data":"YQ=="},
            "demo-2.0.0.tgz": {"data":"Yg=="}
        }
    });
    let response = app
        .oneshot(
            Request::put("/demo")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("npm publish request"),
        )
        .await
        .expect("npm publish response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        store
            .get_package("demo")
            .await
            .expect("query package")
            .is_none()
    );
}

#[tokio::test]
async fn publish_uses_canonical_tarball_route_for_arbitrary_attachment_name() {
    let temp = tempfile::tempdir().expect("temporary registry directory");
    let store = std::sync::Arc::new(
        crate::storage::RegistryStore::new(temp.path())
            .await
            .expect("registry store"),
    );
    let auth = std::sync::Arc::new(crate::auth::AuthService::new(None, store.clone()));
    let app: Router = routes().with_state((store.clone(), auth));
    let body = serde_json::json!({
        "name": "demo",
        "versions": {
            "1.0.0": {"name":"demo","version":"1.0.0","dist":{"integrity":"sha512-old","shasum":"old","tarball":"http://old/demo-1.0.0.tgz"}}
        },
        "_attachments": {"release.tgz": {"data":"YQ=="}}
    });
    let response = app
        .clone()
        .oneshot(
            Request::put("/demo")
                .header("content-type", "application/json")
                .header("host", "registry.example")
                .body(Body::from(body.to_string()))
                .expect("npm publish request"),
        )
        .await
        .expect("npm publish response");
    assert_eq!(response.status(), StatusCode::OK);
    let published: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 16 * 1024)
            .await
            .expect("read published package body"),
    )
    .expect("parse published package body");
    assert_eq!(
        published["versions"]["1.0.0"]["dist"]["tarball"],
        "http://registry.example/demo/-/demo-1.0.0.tgz"
    );

    let download = app
        .oneshot(
            Request::get("/demo/-/demo-1.0.0.tgz")
                .body(Body::empty())
                .expect("canonical tarball request"),
        )
        .await
        .expect("canonical tarball response");
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(
        axum::body::to_bytes(download.into_body(), 16 * 1024)
            .await
            .expect("read downloaded tarball"),
        "a"
    );
}
