#![allow(clippy::unwrap_used)]
//! Registry server tests
//! (Tests: auth, model serialization, storage init, npm route matching — per RULE §5)

use mgc_registry_server::auth::AuthService;
use mgc_registry_server::model::Package;
use mgc_registry_server::storage::RegistryStore;
use std::collections::HashMap;

#[tokio::test]
async fn auth_service_creation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = AuthService::new(Some("admin-token".to_string()), store);
    assert_eq!(auth.admin_token, Some("admin-token".to_string()));
}

#[tokio::test]
async fn admin_token_verification() {
    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = AuthService::new(Some("admin-token".to_string()), store);
    let user = auth.verify_token("admin-token");
    assert!(user.is_some());
    assert!(user.unwrap().is_admin);
}

#[test]
fn package_serializes() {
    let pkg = Package {
        name: "test-pkg".to_string(),
        description: None,
        versions: Default::default(),
        dist_tags: Default::default(),
        maintainers: vec![],
        time: HashMap::from([
            (
                "created".to_string(),
                "2024-01-01T00:00:00.000Z".to_string(),
            ),
            (
                "modified".to_string(),
                "2024-01-01T00:00:00.000Z".to_string(),
            ),
        ]),
        private: true,
    };
    let json = serde_json::to_string(&pkg).unwrap();
    assert!(json.contains("test-pkg"));
}

#[tokio::test]
async fn test_store_creation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = RegistryStore::new(temp_dir.path()).await.unwrap();
    assert!(temp_dir.path().join("registry.db").exists());
    assert!(temp_dir.path().join("blobs").is_dir());
    drop(store);
}

#[tokio::test]
async fn audit_log_writes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = RegistryStore::new(temp_dir.path()).await.unwrap();
    store
        .audit("publish", "pkg-a", Some("1.0.0"), Some("user1"))
        .await
        .unwrap();
    store
        .audit("delete", "pkg-a", Some("1.0.0"), None)
        .await
        .unwrap();
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}?mode=rwc",
        temp_dir.path().join("registry.db").display()
    ))
    .await
    .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE event_type IN ('publish','delete')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
    pool.close().await;
}

#[tokio::test]
async fn rbac_role_controls_publish() {
    use mgc_registry_server::auth::{AuthService, UserRole};
    let temp_dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(temp_dir.path()).await.unwrap());
    let auth = AuthService::new(None, store);

    let viewer = mgc_registry_server::auth::User {
        name: "viewer1".into(),
        is_admin: false,
        role: UserRole::Viewer,
        scopes: vec!["@org/*".into()],
        password: None,
        email: None,
    };
    let publisher = mgc_registry_server::auth::User {
        name: "pub1".into(),
        is_admin: false,
        role: UserRole::Publisher,
        scopes: vec!["@org/*".into()],
        password: None,
        email: None,
    };
    let admin = mgc_registry_server::auth::User {
        name: "admin1".into(),
        is_admin: true,
        role: UserRole::Admin,
        scopes: vec![],
        password: None,
        email: None,
    };

    assert!(!auth.can_publish(&viewer, "@org/x"));
    assert!(!auth.can_publish(&viewer, "@other/x"));
    assert!(auth.can_publish(&publisher, "@org/x"));
    assert!(!auth.can_publish(&publisher, "@other/x"));
    assert!(auth.can_publish(&admin, "@other/x"));
    assert!(auth.can_access(&viewer, "@org/x"));
    assert!(!auth.can_access(&viewer, "@other/x"));
}

/// Param routes use matchit 0.7 `:param` syntax (not `{param}` which is
/// matchit 0.8 / axum 0.8). Guard against silent 404 regression.
#[tokio::test]
async fn param_route_minimal() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::{Router, routing::get};
    use tower::ServiceExt;

    let app = Router::new().route("/a/:x", get(|| async {}));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/a/hello")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Route matching: a matched route returns a handler-level status
/// (method/body/auth error or handler 404), never the router-level 404
/// of an unmatched path. Route syntax `:param` per matchit 0.7.
#[tokio::test]
async fn npm_routes_match() {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = std::sync::Arc::new(AuthService::new(Some("adm".to_string()), store.clone()));
    let app = Router::new()
        .merge(mgc_registry_server::npm::routes())
        .with_state((store, auth));

    let cases = [
        // matched, handler rejects body (no Content-Type) -> 415
        ("/-/user/bob", "PUT", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        // matched, package missing -> handler 404
        ("/-/package/x/dist-tags", "GET", StatusCode::NOT_FOUND),
        // matched, body rejected -> 415
        (
            "/-/package/x/dist-tags/latest",
            "PUT",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        // matched, no auth -> 401
        ("/-/whoami", "GET", StatusCode::UNAUTHORIZED),
        // matched, works without data
        ("/-/v1/search?q=x", "GET", StatusCode::OK),
        // matched, package missing -> handler 404
        ("/npm/foo", "GET", StatusCode::NOT_FOUND),
    ];
    for (path, method, expected) in cases {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .method(method)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            expected,
            "{} {} -> {} (expected {})",
            method,
            path,
            resp.status(),
            expected
        );
    }
}

#[tokio::test]
async fn scoped_publisher_cannot_mount_blob_from_another_repository() {
    use axum::{body::Body, http::Request};
    use mgc_registry_server::{
        auth::{AuthService, User, UserRole},
        oci,
    };
    use sha2::{Digest, Sha256};
    use std::sync::Arc;
    use tower::ServiceExt;

    let temp_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(temp_dir.path()).await.unwrap());
    let source = "ai/private";
    let destination = "ai/destination";
    let bytes = b"private layer";
    let digest = format!("sha256:{}", hex::encode(Sha256::digest(bytes)));
    store.put_oci_blob(source, &digest, bytes).await.unwrap();

    let auth = Arc::new(AuthService::new(Some("admin-token".into()), store.clone()));
    auth.add_user(
        "destination-token".into(),
        User {
            name: "destination-publisher".into(),
            is_admin: false,
            role: UserRole::Publisher,
            scopes: vec!["oci:ai/destination".into()],
            password: None,
            email: None,
        },
    );
    let app = oci::routes().with_state((store.clone(), auth));
    let uri = format!("/v2/{destination}/blobs/uploads/?mount={digest}&from={source}");

    let denied = app
        .clone()
        .oneshot(
            Request::post(&uri)
                .header("authorization", "Bearer destination-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), axum::http::StatusCode::FORBIDDEN);
    assert!(
        store
            .get_oci_blob(destination, &digest)
            .await
            .unwrap()
            .is_none()
    );

    let get_bypass = app
        .clone()
        .oneshot(
            Request::get(&uri)
                .header("authorization", "Bearer destination-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_bypass.status(), axum::http::StatusCode::FORBIDDEN);
    assert!(
        store
            .get_oci_blob(destination, &digest)
            .await
            .unwrap()
            .is_none()
    );

    let get_without_mount = app
        .clone()
        .oneshot(
            Request::get(format!("/v2/{destination}/blobs/uploads/"))
                .header("authorization", "Bearer admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        get_without_mount.status(),
        axum::http::StatusCode::METHOD_NOT_ALLOWED
    );

    let allowed = app
        .oneshot(
            Request::post(&uri)
                .header("authorization", "Bearer admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), axum::http::StatusCode::CREATED);
    assert_eq!(
        store.get_oci_blob(destination, &digest).await.unwrap(),
        Some(bytes.to_vec())
    );
}

#[tokio::test]
async fn oci_manifest_requires_schema_and_all_verified_blobs_before_publish() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use mgc_registry_server::{auth::AuthService, oci};
    use sha2::{Digest, Sha256};
    use std::sync::Arc;
    use tower::ServiceExt;

    let temp_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(temp_dir.path()).await.unwrap());
    let auth = Arc::new(AuthService::new(Some("admin-token".into()), store.clone()));
    let app = oci::routes().with_state((store.clone(), auth));

    let invalid = app
        .clone()
        .oneshot(
            Request::put("/v2/ai/models/weights/manifests/v1")
                .header("authorization", "Bearer admin-token")
                .header("content-type", "application/vnd.oci.image.manifest.v1+json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let config = b"config bytes";
    let layer = b"artifact bytes";
    let config_digest = format!("sha256:{}", hex::encode(Sha256::digest(config)));
    let layer_digest = format!("sha256:{}", hex::encode(Sha256::digest(layer)));
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {"mediaType": "application/vnd.oci.empty.v1+json", "digest": config_digest, "size": config.len()},
        "layers": [{"mediaType": "application/vnd.oci.image.layer.v1.tar", "digest": layer_digest, "size": layer.len()}]
    });
    let missing_blobs = app
        .clone()
        .oneshot(
            Request::put("/v2/ai/models/weights/manifests/v1")
                .header("authorization", "Bearer admin-token")
                .header("content-type", "application/vnd.oci.image.manifest.v1+json")
                .body(Body::from(manifest.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_blobs.status(), StatusCode::BAD_REQUEST);
    assert!(
        store
            .get_oci_manifest("ai/models/weights", "v1")
            .await
            .unwrap()
            .is_none()
    );

    store
        .put_oci_blob("ai/models/weights", &config_digest, config)
        .await
        .unwrap();
    store
        .put_oci_blob("ai/models/weights", &layer_digest, layer)
        .await
        .unwrap();
    let invalid_reference = app
        .clone()
        .oneshot(
            Request::put("/v2/ai/models/weights/manifests/bad%0Aref")
                .header("authorization", "Bearer admin-token")
                .header("content-type", "application/vnd.oci.image.manifest.v1+json")
                .body(Body::from(manifest.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_reference.status(), StatusCode::BAD_REQUEST);
    assert!(
        store
            .get_oci_manifest("ai/models/weights", "bad\nref")
            .await
            .unwrap()
            .is_none()
    );
    let wrong_digest_reference = format!("sha256:{}", "0".repeat(64));
    let mismatched_digest = app
        .clone()
        .oneshot(
            Request::put(format!(
                "/v2/ai/models/weights/manifests/{wrong_digest_reference}"
            ))
            .header("authorization", "Bearer admin-token")
            .header("content-type", "application/vnd.oci.image.manifest.v1+json")
            .body(Body::from(manifest.to_string()))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mismatched_digest.status(), StatusCode::BAD_REQUEST);
    assert!(
        store
            .get_oci_manifest("ai/models/weights", &wrong_digest_reference)
            .await
            .unwrap()
            .is_none()
    );
    let duplicate_layer = manifest["layers"][0].clone();
    let mut too_many_descriptors = manifest.clone();
    too_many_descriptors["layers"] = serde_json::json!(vec![duplicate_layer; 10_000]);
    let descriptor_budget = app
        .clone()
        .oneshot(
            Request::put("/v2/ai/models/weights/manifests/too-many-descriptors")
                .header("authorization", "Bearer admin-token")
                .header("content-type", "application/vnd.oci.image.manifest.v1+json")
                .body(Body::from(too_many_descriptors.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(descriptor_budget.status(), StatusCode::BAD_REQUEST);
    assert!(
        store
            .get_oci_manifest("ai/models/weights", "too-many-descriptors")
            .await
            .unwrap()
            .is_none()
    );
    let mut wrong_size = manifest.clone();
    wrong_size["layers"][0]["size"] = serde_json::json!(layer.len() + 1);
    let rejected_size = app
        .clone()
        .oneshot(
            Request::put("/v2/ai/models/weights/manifests/v1")
                .header("authorization", "Bearer admin-token")
                .header("content-type", "application/vnd.oci.image.manifest.v1+json")
                .body(Body::from(wrong_size.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected_size.status(), StatusCode::BAD_REQUEST);
    let published = app
        .clone()
        .oneshot(
            Request::put("/v2/ai/models/weights/manifests/v1")
                .header("authorization", "Bearer admin-token")
                .header("content-type", "application/vnd.oci.image.manifest.v1+json")
                .body(Body::from(manifest.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::CREATED);
    assert_eq!(
        published.headers().get("location").unwrap(),
        "/v2/ai/models/weights/manifests/v1"
    );

    let (_, child_digest) = store
        .get_oci_manifest_raw("ai/models/weights", "v1")
        .await
        .unwrap()
        .unwrap();
    let child_size = manifest.to_string().len();
    let index = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.index.v1+json",
        "manifests": [{
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": child_digest.clone(),
            "size": child_size,
            "platform": {"architecture": "amd64", "os": "linux"}
        }]
    });
    let index_published = app
        .clone()
        .oneshot(
            Request::put("/v2/ai/models/weights/manifests/multi")
                .header("authorization", "Bearer admin-token")
                .header("content-type", "application/vnd.oci.image.index.v1+json")
                .body(Body::from(index.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(index_published.status(), StatusCode::CREATED);
    let index_get = app
        .clone()
        .oneshot(
            Request::get("/v2/ai/models/weights/manifests/multi")
                .header("authorization", "Bearer admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(index_get.status(), StatusCode::OK);
    assert_eq!(
        index_get.headers().get("content-type").unwrap(),
        "application/vnd.oci.image.index.v1+json"
    );

    let docker_list = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.docker.distribution.manifest.list.v2+json",
        "manifests": [{
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": child_digest,
            "size": child_size,
            "platform": {"architecture": "arm64", "os": "linux"}
        }]
    });
    let list_published = app
        .clone()
        .oneshot(
            Request::put("/v2/ai/models/weights/manifests/docker-list")
                .header("authorization", "Bearer admin-token")
                .header(
                    "content-type",
                    "application/vnd.docker.distribution.manifest.list.v2+json",
                )
                .body(Body::from(docker_list.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_published.status(), StatusCode::CREATED);
    let list_get = app
        .oneshot(
            Request::get("/v2/ai/models/weights/manifests/docker-list")
                .header("authorization", "Bearer admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_get.status(), StatusCode::OK);
    assert_eq!(
        list_get.headers().get("content-type").unwrap(),
        "application/vnd.docker.distribution.manifest.list.v2+json"
    );
}

#[tokio::test]
async fn oci_chunked_upload_streams_to_verified_blob_store() {
    use axum::{body::Body, http::Request};
    use mgc_registry_server::{auth::AuthService, oci};
    use sha2::{Digest, Sha256};
    use std::sync::Arc;
    use tower::ServiceExt;

    let temp_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(temp_dir.path()).await.unwrap());
    let auth = Arc::new(AuthService::new(Some("admin-token".into()), store.clone()));
    let app = oci::routes().with_state((store.clone(), auth));
    let repository = "ai/models/weights";
    let bytes = b"large layer sent through the chunked route";
    let digest = format!("sha256:{}", hex::encode(Sha256::digest(bytes)));

    let started = app
        .clone()
        .oneshot(
            Request::post(format!("/v2/{repository}/blobs/uploads/"))
                .header("authorization", "Bearer admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(started.status(), axum::http::StatusCode::ACCEPTED);
    let upload_location = started
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let split = bytes.len() / 2;
    let chunked = app
        .clone()
        .oneshot(
            Request::patch(&upload_location)
                .header("authorization", "Bearer admin-token")
                .header("content-range", format!("0-{}", split - 1))
                .body(Body::from(bytes[..split].to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(chunked.status(), axum::http::StatusCode::ACCEPTED);
    assert_eq!(
        chunked.headers().get("range").unwrap(),
        format!("0-{}", split - 1).as_str()
    );

    let wrong_final_offset = app
        .clone()
        .oneshot(
            Request::put(format!("{upload_location}?digest={digest}"))
                .header("authorization", "Bearer admin-token")
                .header("content-range", format!("0-{}", bytes.len() - split - 1))
                .body(Body::from(bytes[split..].to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        wrong_final_offset.status(),
        axum::http::StatusCode::RANGE_NOT_SATISFIABLE
    );

    let finished = app
        .oneshot(
            Request::put(format!("{upload_location}?digest={digest}"))
                .header("authorization", "Bearer admin-token")
                .header("content-range", format!("{split}-{}", bytes.len() - 1))
                .body(Body::from(bytes[split..].to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(finished.status(), axum::http::StatusCode::CREATED);
    assert_eq!(
        store.get_oci_blob(repository, &digest).await.unwrap(),
        Some(bytes.to_vec())
    );
    assert!(
        store
            .oci_upload_path(repository, upload_location.rsplit('/').next().unwrap())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn oci_upload_rejects_invalid_ranges_without_appending() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use mgc_registry_server::{auth::AuthService, oci};
    use std::sync::Arc;
    use tower::ServiceExt;

    let temp_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(temp_dir.path()).await.unwrap());
    let repository = "ai/models/weights";
    let auth = Arc::new(AuthService::new(Some("admin-token".into()), store.clone()));
    let app = oci::routes().with_state((store.clone(), auth));
    let started = app
        .clone()
        .oneshot(
            Request::post(format!("/v2/{repository}/blobs/uploads/"))
                .header("authorization", "Bearer admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(started.status(), StatusCode::ACCEPTED);
    let location = started.headers().get("location").unwrap().to_str().unwrap();
    let uuid = location.rsplit('/').next().unwrap().to_owned();
    assert!(uuid::Uuid::parse_str(&uuid).is_ok());
    let upload_path = store
        .oci_upload_path(repository, &uuid)
        .await
        .unwrap()
        .unwrap();
    let uri = format!("/v2/{repository}/blobs/uploads/{uuid}");

    for range in [
        None,
        Some("0-99"),
        Some("1-3"),
        Some("bytes 0-2/*"),
        Some("garbage"),
    ] {
        let mut request = Request::patch(&uri)
            .header("authorization", "Bearer admin-token")
            .body(Body::from("abc"))
            .unwrap();
        if let Some(range) = range {
            request
                .headers_mut()
                .insert("content-range", range.parse().unwrap());
        }
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert!(!tokio::fs::try_exists(&upload_path).await.unwrap());
    }

    let accepted = app
        .clone()
        .oneshot(
            Request::patch(&uri)
                .header("authorization", "Bearer admin-token")
                .header("content-range", "0-2")
                .body(Body::from("abc"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    assert_eq!(accepted.headers().get("range").unwrap(), "0-2");
    assert_eq!(accepted.headers().get("location").unwrap(), uri.as_str());
    assert_eq!(tokio::fs::metadata(&upload_path).await.unwrap().len(), 3);

    let request = |body: &'static str| {
        Request::patch(&uri)
            .header("authorization", "Bearer admin-token")
            .header("content-range", "3-5")
            .body(Body::from(body))
            .unwrap()
    };
    let (first, second) = tokio::join!(
        app.clone().oneshot(request("def")),
        app.oneshot(request("xyz"))
    );
    let statuses = [first.unwrap().status(), second.unwrap().status()];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::ACCEPTED)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::RANGE_NOT_SATISFIABLE)
            .count(),
        1
    );
    let final_bytes = tokio::fs::read(&upload_path).await.unwrap();
    assert_eq!(final_bytes.len(), 6);
    assert!(final_bytes == b"abcdef" || final_bytes == b"abcxyz");
    assert!(!store.finish_oci_upload(repository, &uuid, 3).await.unwrap());
    assert!(store.finish_oci_upload(repository, &uuid, 6).await.unwrap());
    assert!(!tokio::fs::try_exists(&upload_path).await.unwrap());
}

#[tokio::test]
async fn pypi_upload_index_download() {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = std::sync::Arc::new(AuthService::new(Some("adm".to_string()), store.clone()));
    let app = Router::new()
        .merge(mgc_registry_server::pypi::routes())
        .with_state((store, auth));

    // Upload wheel content (ASCII — multipart test body là string literal)
    let wheel = b"dummy wheel content for test";
    let mut hasher = Sha256::new();
    hasher.update(wheel);
    let digest = format!("sha256:{:x}", hasher.finalize());

    // multipart upload → twine format
    let boundary = "MGBTEST";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\":action\"\r\n\r\nfile_upload\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\ndemo-pkg\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"version\"\r\n\r\n1.0.0\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"sha256_digest\"\r\n\r\n{sha}\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"requires_python\"\r\n\r\n>=3.11\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"content\"; filename=\"demo_pkg-1.0.0-py3-none-any.whl\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n{data}\r\n\
         --{b}--\r\n",
        b = boundary,
        sha = digest.trim_start_matches("sha256:"),
        data = String::from_utf8_lossy(wheel)
    );

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pypi/legacy/")
                .method("POST")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header("authorization", "Bearer adm")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        upload.status(),
        StatusCode::OK,
        "upload: {}",
        upload.status()
    );

    // Simple index (PEP 503 HTML — pip cũ/mới đều đọc được)
    let idx = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pypi/simple/demo-pkg/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idx.status(), StatusCode::OK, "index: {}", idx.status());
    let idx_body = axum::body::to_bytes(idx.into_body(), 8192).await.unwrap();
    let idx_html = String::from_utf8(idx_body.to_vec()).unwrap();
    assert!(
        idx_html.contains("Links for demo-pkg"),
        "index HTML: {idx_html}"
    );
    assert!(
        idx_html.contains("/pypi/packages/demo-pkg/demo_pkg-1.0.0-py3-none-any.whl"),
        "index link: {idx_html}"
    );

    // Download file
    let dl = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pypi/packages/demo-pkg/demo_pkg-1.0.0-py3-none-any.whl")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dl.status(), StatusCode::OK, "download: {}", dl.status());
    let dl_body = axum::body::to_bytes(dl.into_body(), 8192).await.unwrap();
    assert_eq!(&dl_body[..], wheel);

    // sha256 mismatch → rejected
    let bad = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nbad-pkg\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"version\"\r\n\r\n1.0.0\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"sha256_digest\"\r\n\r\ndeadbeef\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"content\"; filename=\"bad.whl\"\r\n\r\nwhatever\r\n\
         --{b}--\r\n",
        b = boundary
    );
    let bad_req = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pypi/legacy/")
                .method("POST")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header("authorization", "Bearer adm")
                .body(Body::from(bad))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        bad_req.status(),
        StatusCode::BAD_REQUEST,
        "sha mismatch rejected"
    );
}

#[tokio::test]
async fn users_persist_across_restart() {
    use mgc_registry_server::auth::User;

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_path_buf();

    // First "restart": create store + auth, add user, drop
    {
        let store = std::sync::Arc::new(RegistryStore::new(&path).await.unwrap());
        let auth = AuthService::new(None, store.clone());
        auth.add_user(
            "tok-1".to_string(),
            User {
                name: "alice".to_string(),
                is_admin: false,
                role: mgc_registry_server::auth::UserRole::Publisher,
                scopes: vec!["@org/*".to_string()],
                password: Some("pw".to_string()),
                email: None,
            },
        );
        // await persist (spawn) — poll DB directly
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        drop(auth);
        drop(store);
    }

    // Second "restart": new AuthService từ cùng store_dir → user phải còn
    let store = std::sync::Arc::new(RegistryStore::new(&path).await.unwrap());
    let auth = AuthService::new(None, store);
    auth.load_from_db().await.unwrap();
    let user = auth
        .verify_token("tok-1")
        .expect("user token survives restart");
    assert_eq!(user.name, "alice");
    assert_eq!(user.scopes, vec!["@org/*".to_string()]);

    // delete → DB cũng mất
    assert!(auth.remove_user("alice").await.unwrap());
    let gone = auth.verify_token("tok-1");
    assert!(gone.is_none());
}

#[tokio::test]
async fn oci_routes_match() {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = std::sync::Arc::new(AuthService::new(Some("adm".to_string()), store.clone()));
    let app = Router::new()
        .merge(mgc_registry_server::oci::routes())
        .with_state((store, auth));

    // HEAD blob on missing digest -> 404 from handler (route matched)
    let head = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v2/ai/mymodel/blobs/sha256:deadbeef")
                .method("HEAD")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        head.status(),
        StatusCode::NOT_FOUND,
        "blob HEAD: {}",
        head.status()
    );

    // POST uploads -> 202 with location — POST tạo session upload.
    let post = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v2/ai/mymodel/blobs/uploads/")
                .method("POST")
                .header("authorization", "Bearer adm")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        post.status(),
        StatusCode::ACCEPTED,
        "upload start: {}",
        post.status()
    );

    // tags list -> 200
    let tags = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v2/ai/mymodel/tags/list")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        tags.status(),
        StatusCode::OK,
        "tags/list: {}",
        tags.status()
    );

    // catalog -> 200
    let cat = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v2/_catalog")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cat.status(), StatusCode::OK, "catalog: {}", cat.status());
}

#[test]
fn scope_glob_matches() {
    use mgc_registry_server::auth::scope_matches;
    assert!(scope_matches("*", "anything"));
    assert!(scope_matches("@magicore/*", "@magicore/core"));
    assert!(scope_matches("@magicore/*", "@magicore/core/extra"));
    assert!(!scope_matches("@magicore/*", "@other/pkg"));
    assert!(scope_matches("mypkg", "mypkg"));
    assert!(!scope_matches("mypkg", "mypkg2"));
    assert!(scope_matches("myapp/*", "myapp"));
}

#[tokio::test]
async fn npm_delete_version_and_oci_tags_catalog() {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use sha2::Digest as _;
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = std::sync::Arc::new(AuthService::new(Some("adm".to_string()), store.clone()));
    let app = Router::new()
        .merge(mgc_registry_server::npm::routes())
        .merge(mgc_registry_server::oci::routes())
        .with_state((store.clone(), auth));

    // npm: publish 1 version
    let pkg = serde_json::json!({
        "name": "demo-pkg",
        "maintainers": [],
        "versions": {
            "1.0.0": {
                "name": "demo-pkg",
                "version": "1.0.0",
                "_id": "demo-pkg@1.0.0",
                "_rev": "1",
                "dist": {"tarball": "http://x/demo-pkg-1.0.0.tgz", "shasum": "", "integrity": ""}
            }
        },
        "dist-tags": {"latest": "1.0.0"},
        "time": {"created":"","modified":""},
        "private": true
    });
    let put = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/npm/demo-pkg")
                .method("PUT")
                .header("content-type", "application/json")
                .header("authorization", "Bearer adm")
                .body(Body::from(pkg.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK, "publish: {}", put.status());

    // DELETE version → package hết version → package bị xóa luôn
    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/npm/demo-pkg/-/demo-pkg-1.0.0.tgz")
                .method("DELETE")
                .header("authorization", "Bearer adm")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        del.status(),
        StatusCode::NO_CONTENT,
        "delete version: {}",
        del.status()
    );

    let get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/npm/demo-pkg")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        get.status(),
        StatusCode::NOT_FOUND,
        "package gone after delete"
    );

    // OCI: put manifest → tags/list + catalog thấy repo
    let config = b"test OCI config";
    let digest = format!("sha256:{}", hex::encode(sha2::Sha256::digest(config)));
    store.put_oci_blob("ai/m1", &digest, config).await.unwrap();
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {"mediaType": "application/vnd.oci.image.config.v1+json", "digest": digest, "size": config.len()},
        "layers": []
    })
    .to_string();
    let mput = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v2/ai/m1/manifests/1.0.0")
                .method("PUT")
                .header("content-type", "application/vnd.oci.image.manifest.v1+json")
                .header("authorization", "Bearer adm")
                .body(Body::from(manifest))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        mput.status(),
        StatusCode::CREATED,
        "manifest put: {}",
        mput.status()
    );

    let tags = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v2/ai/m1/tags/list")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let tags_body = axum::body::to_bytes(tags.into_body(), 4096).await.unwrap();
    let tags_json: serde_json::Value = serde_json::from_slice(&tags_body).unwrap();
    assert_eq!(
        tags_json["tags"],
        serde_json::json!(["1.0.0"]),
        "tags: {}",
        tags_json
    );

    let cat = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v2/_catalog")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cat_body = axum::body::to_bytes(cat.into_body(), 4096).await.unwrap();
    let cat_json: serde_json::Value = serde_json::from_slice(&cat_body).unwrap();
    assert_eq!(
        cat_json["repositories"],
        serde_json::json!(["ai/m1"]),
        "catalog: {}",
        cat_json
    );
}

#[tokio::test]
async fn registry_write_authorization_enforces_package_and_role() {
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use mgc_registry_server::auth::{AuthService, User, UserRole};

    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = AuthService::new(Some("admin-token".into()), store);
    let publisher = User {
        name: "publisher".into(),
        is_admin: false,
        role: UserRole::Publisher,
        scopes: vec!["@team/*".into()],
        password: Some("pw".into()),
        email: None,
    };
    auth.register_user("publisher-token".into(), publisher)
        .await
        .unwrap();

    let mut headers = HeaderMap::new();
    assert_eq!(
        auth.authorize_write(&headers, "@team/pkg").await,
        Err(StatusCode::UNAUTHORIZED)
    );
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer publisher-token"),
    );
    assert_eq!(auth.authorize_write(&headers, "@team/pkg").await, Ok(()));
    assert_eq!(
        auth.authorize_write(&headers, "@other/pkg").await,
        Err(StatusCode::FORBIDDEN)
    );

    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer admin-token"),
    );
    assert_eq!(auth.authorize_write(&headers, "@other/pkg").await, Ok(()));
}

#[tokio::test]
async fn public_registration_cannot_escalate_or_replace_existing_user() {
    use axum::http::StatusCode;
    use mgc_registry_server::auth::{AuthService, User, UserRole};

    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = AuthService::new(Some("admin-token".into()), store);
    let viewer = User {
        name: "same-name".into(),
        is_admin: false,
        role: UserRole::Viewer,
        scopes: vec![],
        password: Some("pw".into()),
        email: None,
    };
    auth.register_user("first-token".into(), viewer)
        .await
        .unwrap();
    let attacker = User {
        name: "same-name".into(),
        is_admin: true,
        role: UserRole::Admin,
        scopes: vec!["*".into()],
        password: Some("attacker".into()),
        email: None,
    };
    assert_eq!(
        auth.register_user("second-token".into(), attacker).await,
        Err(StatusCode::CONFLICT)
    );
    assert!(auth.verify_token("first-token").is_some());
    assert!(auth.verify_token("second-token").is_none());
}

#[tokio::test]
async fn account_mutations_require_configured_admin() {
    use axum::{Router, body::Body, http::Request};
    use mgc_registry_server::auth::{AuthService, User, UserRole};
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = std::sync::Arc::new(AuthService::new(Some("admin-token".into()), store.clone()));
    for (token, name, role) in [
        ("publisher-token", "publisher", UserRole::Publisher),
        ("victim-token", "victim", UserRole::Viewer),
        ("second-token", "second", UserRole::Viewer),
    ] {
        auth.register_user(
            token.into(),
            User {
                name: name.into(),
                is_admin: false,
                role,
                scopes: vec![],
                password: Some("pw".into()),
                email: None,
            },
        )
        .await
        .unwrap();
    }
    let app = Router::new()
        .merge(mgc_registry_server::npm::routes())
        .with_state((store.clone(), auth.clone()));

    for (uri, authorization, expected) in [
        ("/-/user/victim", None, axum::http::StatusCode::UNAUTHORIZED),
        (
            "/-/user/victim",
            Some("Bearer publisher-token"),
            axum::http::StatusCode::FORBIDDEN,
        ),
        (
            "/-/user/token/second-token",
            Some("Bearer publisher-token"),
            axum::http::StatusCode::FORBIDDEN,
        ),
    ] {
        let mut request = Request::builder().method("DELETE").uri(uri);
        if let Some(value) = authorization {
            request = request.header("authorization", value);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "DELETE {uri}");
    }
    assert!(auth.verify_token("victim-token").is_some());
    assert!(auth.verify_token("second-token").is_some());

    let deleted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/-/user/victim")
                .header("authorization", "Bearer admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), axum::http::StatusCode::NO_CONTENT);
    assert!(auth.verify_token("victim-token").is_none());

    let revoked = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/-/user/token/second-token")
                .header("authorization", "Bearer admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), axum::http::StatusCode::NO_CONTENT);
    assert!(auth.verify_token("second-token").is_none());

    let open_auth = AuthService::new(None, store);
    assert_eq!(
        open_auth.authorize_admin(&axum::http::HeaderMap::new()),
        Err(axum::http::StatusCode::FORBIDDEN)
    );
}

#[tokio::test]
async fn npm_routes_reject_public_privilege_escalation_and_cross_package_upload() {
    use axum::{
        Extension, Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
    };
    use mgc_registry_server::{
        auth::{AuthService, User, UserRole, auth_middleware},
        npm,
    };
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = std::sync::Arc::new(AuthService::new(Some("admin-token".into()), store.clone()));
    auth.register_user(
        "team-token".into(),
        User {
            name: "team-publisher".into(),
            is_admin: false,
            role: UserRole::Publisher,
            scopes: vec!["@team/*".into()],
            password: Some("pw".into()),
            email: None,
        },
    )
    .await
    .unwrap();
    let app = Router::new()
        .merge(npm::routes())
        .route_layer(middleware::from_fn(auth_middleware))
        .layer(Extension(auth.clone()))
        .with_state((store.clone(), auth));

    let registration = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/-/user/attacker")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"password":"pw","role":"publisher","scopes":["*"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(registration.status(), StatusCode::FORBIDDEN);

    let upload = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/@other/pkg/-/pkg-1.tgz")
                .header("authorization", "Bearer team-token")
                .body(Body::from("should not be stored"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::FORBIDDEN);
    assert!(
        std::fs::read_dir(tmp.path().join("blobs"))
            .unwrap()
            .next()
            .is_none()
    );
}
