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
    use axum::{routing::get, Router};
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
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::Router;
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
async fn pypi_upload_index_download() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::Router;
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
        idx_html.contains("../../packages/demo-pkg/demo_pkg-1.0.0-py3-none-any.whl"),
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
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::Router;
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

    // POST uploads -> 200 with location (route matched)
    let post = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v2/ai/mymodel/blobs/uploads/")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        post.status(),
        StatusCode::OK,
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
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::Router;
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = std::sync::Arc::new(AuthService::new(Some("adm".to_string()), store.clone()));
    let app = Router::new()
        .merge(mgc_registry_server::npm::routes())
        .merge(mgc_registry_server::oci::routes())
        .with_state((store, auth));

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
    let digest = "sha256:aaaa";
    let manifest = format!(
        r#"{{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{{"mediaType":"t","digest":"{}","size":1}},"layers":[]}}"#,
        digest
    );
    let mput = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v2/ai/m1/manifests/1.0.0")
                .method("PUT")
                .header("content-type", "application/vnd.oci.image.manifest.v1+json")
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
