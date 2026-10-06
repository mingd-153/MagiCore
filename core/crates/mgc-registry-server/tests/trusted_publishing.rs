#![allow(clippy::unwrap_used)]
//! Trusted publishing storage, authorization, and route tests.
//! Kiểm tra lưu trữ, phân quyền và route trusted publishing.

use axum::{Router, body::Body, http::Request};
use base64::Engine as _;
use mgc_crypto::KeyPair;
use mgc_oidc::{OidcClaims, claims::Audience};
use mgc_registry_server::{
    AppState,
    auth::AuthService,
    model::Package,
    storage::RegistryStore,
    trusted::{self, TrustedPublishingConfig},
};
use std::{collections::HashMap, sync::Arc};
use tower::ServiceExt as _;

fn package(name: &str, version: &str, digest: &str) -> Package {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "dist-tags": {"latest": version},
        "versions": {
            version: {
                "name": name,
                "version": version,
                "dist": {
                    "integrity": digest,
                    "shasum": "",
                    "tarball": "https://registry.example/pkg.tgz"
                }
            }
        }
    }))
    .unwrap()
}

fn claims() -> OidcClaims {
    OidcClaims {
        iss: "https://token.actions.githubusercontent.com".into(),
        sub: "repo:acme/widgets:ref:refs/heads/main".into(),
        aud: Audience::Single("https://registry.example".into()),
        exp: 1_900_000_000,
        iat: 1_800_000_000,
        repository: Some("acme/widgets".into()),
        job_workflow_ref: Some("acme/widgets/.github/workflows/publish.yml@refs/heads/main".into()),
        workflow_ref: Some("acme/widgets/.github/workflows/publish.yml@refs/heads/main".into()),
        event_name: Some("push".into()),
    }
}

#[test]
fn repository_binding_rejects_pull_request_claims_even_without_event_name() {
    let mut claims = claims();
    claims.sub = "repo:acme/widgets:pull_request".into();
    claims.event_name = None;
    assert!(!trusted::claims_match_repository_binding(
        &claims,
        "acme/widgets"
    ));

    claims.sub = "repo:acme/widgets:ref:refs/heads/main".into();
    claims.event_name = Some("pull_request_target".into());
    assert!(!trusted::claims_match_repository_binding(
        &claims,
        "acme/widgets"
    ));

    claims.event_name = Some("push".into());
    assert!(trusted::claims_match_repository_binding(
        &claims,
        "acme/widgets"
    ));
}

fn valid_digest() -> String {
    format!(
        "sha512-{}",
        base64::engine::general_purpose::STANDARD.encode([7u8; 64])
    )
}

#[tokio::test]
async fn pypi_aliases_share_one_pep503_storage_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let store = RegistryStore::new(tmp.path()).await.unwrap();
    let file = mgc_registry_server::model::PypiFile {
        name: "Flask_Test.pkg".into(),
        version: "1.0.0".into(),
        filename: "flask_test_pkg-1.0.0.whl".into(),
        digest: format!("sha256:{}", "ab".repeat(32)),
        size: 1,
        requires_python: None,
    };

    store.put_pypi_file(&file).await.unwrap();

    for alias in ["Flask_Test.pkg", "flask-test-pkg", "FLASK---TEST___PKG"] {
        let files = store.get_pypi_files(alias).await.unwrap();
        assert_eq!(
            files.len(),
            1,
            "PyPI alias {alias} must resolve the same project"
        );
        assert_eq!(files[0].name, "flask-test-pkg");
        assert_eq!(
            store
                .get_pypi_file_digest(alias, &file.filename)
                .await
                .unwrap()
                .as_deref(),
            Some(file.digest.as_str())
        );
    }
}

fn public_attestations(pkg: &Package) -> HashMap<String, (String, serde_json::Value)> {
    pkg.versions
        .keys()
        .map(|version| {
            (
                version.clone(),
                ("07".repeat(32), serde_json::json!({"bundle": true})),
            )
        })
        .collect()
}

#[tokio::test]
async fn trusted_attestation_rejects_non_npm_sha512_integrity() {
    let tmp = tempfile::tempdir().unwrap();
    let store = RegistryStore::new(tmp.path()).await.unwrap();
    store
        .set_trusted_publisher("@acme/invalid-digest", "acme/widgets")
        .await
        .unwrap();
    let key = KeyPair::generate().unwrap();
    let invalid_npm_digest = format!("sha256:{}", "07".repeat(32));
    let pkg = package("@acme/invalid-digest", "1.0.0", &invalid_npm_digest);
    let attestations = public_attestations(&pkg);

    assert!(
        store
            .put_trusted_package(&pkg, &claims(), &key, 1, &attestations)
            .await
            .is_err()
    );
    assert!(
        store
            .get_package("@acme/invalid-digest")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn trusted_attestation_requires_matching_active_repository_binding() {
    let tmp = tempfile::tempdir().unwrap();
    let store = RegistryStore::new(tmp.path()).await.unwrap();
    store
        .set_trusted_publisher("@acme/mismatched", "acme/widgets")
        .await
        .unwrap();
    let key = KeyPair::generate().unwrap();
    let mut forged_claims = claims();
    forged_claims.repository = Some("attacker/fork".into());
    forged_claims.sub = "repo:attacker/fork:ref:refs/heads/main".into();
    let pkg = package("@acme/mismatched", "1.0.0", &valid_digest());
    let attestations = public_attestations(&pkg);

    assert!(
        store
            .put_trusted_package(&pkg, &forged_claims, &key, 1, &attestations)
            .await
            .is_err()
    );
    assert!(
        store
            .get_package("@acme/mismatched")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn trusted_attestation_rejects_pull_request_subjects_for_repository_bindings() {
    let tmp = tempfile::tempdir().unwrap();
    let store = RegistryStore::new(tmp.path()).await.unwrap();
    store
        .set_trusted_publisher("pkg", "acme/widgets")
        .await
        .unwrap();
    let key = KeyPair::generate().unwrap();
    let pkg = package("pkg", "1.0.0", &valid_digest());
    let mut pull_request_claims = claims();
    pull_request_claims.sub = "repo:acme/widgets:pull_request".into();
    pull_request_claims.event_name = Some("pull_request".into());

    assert!(
        store
            .put_trusted_package(
                &pkg,
                &pull_request_claims,
                &key,
                1,
                &public_attestations(&pkg),
            )
            .await
            .is_err(),
        "repository-wide bindings must not accept pull-request workloads"
    );
}

#[tokio::test]
async fn legacy_package_storage_refuses_a_trusted_binding() {
    let tmp = tempfile::tempdir().unwrap();
    let store = RegistryStore::new(tmp.path()).await.unwrap();
    store
        .set_trusted_publisher("@acme/bound", "acme/widgets")
        .await
        .unwrap();
    let pkg = package("@acme/bound", "1.0.0", &valid_digest());

    assert!(store.put_package(&pkg).await.is_err());
    assert!(store.get_package("@acme/bound").await.unwrap().is_none());
}

#[tokio::test]
async fn trusted_npm_storage_rejects_namespaced_non_npm_package_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let store = RegistryStore::new(tmp.path()).await.unwrap();
    let key = KeyPair::generate().unwrap();
    let invalid = package("pypi:foo", "1.0.0", &format!("sha512-{}", "A".repeat(88)));
    assert!(
        store
            .put_trusted_package(&invalid, &claims(), &key, 1, &HashMap::new())
            .await
            .is_err()
    );
}

#[test]
fn trusted_protocol_scopes_are_namespaced_and_validated() {
    assert_eq!(
        trusted::scoped_package("npm", "@Acme/widgets").expect("valid npm scoped package"),
        "@Acme/widgets"
    );
    assert_eq!(
        trusted::scoped_package("pypi", "Flask_Test.pkg").expect("valid PyPI project name"),
        "pypi:flask-test-pkg"
    );
    assert_eq!(
        trusted::scoped_package("oci", "acme/models/worker").expect("valid OCI image name"),
        "oci:acme/models/worker"
    );
    assert!(trusted::scoped_package("pypi", "../escape").is_err());
    assert!(trusted::scoped_package("oci", "acme//worker").is_err());
    assert!(trusted::scoped_package("oci", "Acme/worker").is_err());
    assert!(trusted::scoped_package("docker", "acme/worker").is_err());
}

#[test]
fn trusted_oci_repositories_are_canonically_scoped_for_every_core() {
    for core in [
        "web", "ai", "app", "lib", "game", "iot", "cloud", "cicd", "hardware",
    ] {
        assert_eq!(
            trusted::core_scoped_oci_repository(core, "artifacts/model-v1")
                .expect("known core and valid OCI repository"),
            format!("{core}/artifacts/model-v1")
        );
    }
    assert_eq!(
        trusted::core_scoped_oci_repository("clo", "artifacts/model-v1")
            .expect("legacy CLI alias normalizes to canonical core id"),
        "cloud/artifacts/model-v1"
    );
    assert!(trusted::core_scoped_oci_repository("unknown", "artifacts/model-v1").is_err());
    assert!(trusted::core_scoped_oci_repository("ai", "Artifacts/model-v1").is_err());
}

#[tokio::test]
async fn binding_and_signed_chain_persist_and_package_write_is_atomic() {
    let tmp = tempfile::tempdir().unwrap();
    let store = RegistryStore::new(tmp.path()).await.unwrap();
    store
        .set_trusted_publisher("@acme/widgets", "acme/widgets")
        .await
        .unwrap();
    assert_eq!(
        store.trusted_publisher("@acme/widgets").await.unwrap(),
        Some("acme/widgets".into())
    );
    let audit_pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}?mode=rwc",
        tmp.path().join("registry.db").display()
    ))
    .await
    .unwrap();
    let bound_audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE event_type = 'trusted-publisher-bound' AND name = '@acme/widgets' AND user = 'acme/widgets'",
    )
    .fetch_one(&audit_pool)
    .await
    .unwrap();
    assert_eq!(bound_audit_count, 1);
    audit_pool.close().await;

    let key = KeyPair::generate().unwrap();
    let digest = valid_digest();
    let first = package("@acme/widgets", "1.0.0", &digest);
    let first_attestations = public_attestations(&first);
    store
        .put_trusted_package(&first, &claims(), &key, 1, &first_attestations)
        .await
        .unwrap();
    assert!(
        store
            .set_trusted_dist_tag("@acme/widgets", "next", "1.0.0", 1)
            .await
            .unwrap()
    );
    let mut second = package("@acme/widgets", "1.1.0", &digest);
    second.dist_tags.clear();
    second.dist_tags.insert("beta".into(), "1.1.0".into());
    let second_attestations = public_attestations(&second);
    store
        .put_trusted_package(&second, &claims(), &key, 1, &second_attestations)
        .await
        .unwrap();
    let stored = store.get_package("@acme/widgets").await.unwrap().unwrap();
    assert_eq!(
        stored.dist_tags.get("latest").map(String::as_str),
        Some("1.0.0")
    );
    assert_eq!(
        stored.dist_tags.get("beta").map(String::as_str),
        Some("1.1.0")
    );
    assert_eq!(
        stored.dist_tags.get("next").map(String::as_str),
        Some("1.0.0")
    );

    let entries = store.trusted_attestations("@acme/widgets").await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].payload.sequence, 1);
    assert_eq!(entries[0].payload.previous_hash, None);
    assert_eq!(entries[1].payload.sequence, 2);
    assert_eq!(
        entries[1].payload.previous_hash.as_deref(),
        Some(entries[0].entry_hash.as_str())
    );
    let (head_sequence, first_page) = store
        .trusted_attestations_page("@acme/widgets", 0, None, 1)
        .await
        .unwrap();
    assert_eq!(head_sequence, 2);
    assert_eq!(first_page.len(), 1);
    assert_eq!(first_page[0].payload.sequence, 1);
    let (pinned_head, second_page) = store
        .trusted_attestations_page("@acme/widgets", 1, Some(head_sequence), 1)
        .await
        .unwrap();
    assert_eq!(pinned_head, head_sequence);
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].payload.sequence, 2);

    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}?mode=rwc",
        tmp.path().join("registry.db").display()
    ))
    .await
    .unwrap();
    assert!(
        sqlx::query("UPDATE trusted_attestations SET signature = 'tampered'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM trusted_attestations")
            .execute(&pool)
            .await
            .is_err()
    );
    pool.close().await;

    let invalid = package("@acme/invalid", "1.0.0", "");
    let invalid_attestations = public_attestations(&invalid);
    assert!(
        store
            .put_trusted_package(&invalid, &claims(), &key, 1, &invalid_attestations)
            .await
            .is_err()
    );
    assert!(store.get_package("@acme/invalid").await.unwrap().is_none());
    let mut empty = package("@acme/empty", "1.0.0", &valid_digest());
    empty.versions.clear();
    let empty_attestations = public_attestations(&empty);
    assert!(
        store
            .put_trusted_package(&empty, &claims(), &key, 1, &empty_attestations)
            .await
            .is_err()
    );
    assert!(store.get_package("@acme/empty").await.unwrap().is_none());
}

#[tokio::test]
async fn old_binding_generation_cannot_publish_after_repository_cycles_back() {
    let tmp = tempfile::tempdir().unwrap();
    let store = RegistryStore::new(tmp.path()).await.unwrap();
    store
        .set_trusted_publisher("@acme/cycle", "acme/widgets")
        .await
        .unwrap();
    store
        .set_trusted_publisher("@acme/cycle", "other/widgets")
        .await
        .unwrap();
    store
        .set_trusted_publisher("@acme/cycle", "acme/widgets")
        .await
        .unwrap();
    let binding = store
        .trusted_publisher_binding("@acme/cycle")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding, ("acme/widgets".into(), 3));

    let key = KeyPair::generate().unwrap();
    let cycle = package("@acme/cycle", "1.0.0", &valid_digest());
    let cycle_attestations = public_attestations(&cycle);
    assert!(
        store
            .put_trusted_package(&cycle, &claims(), &key, 1, &cycle_attestations,)
            .await
            .is_err()
    );
    assert!(store.get_package("@acme/cycle").await.unwrap().is_none());
}

#[tokio::test]
async fn pypi_and_oci_manifest_are_atomic_with_public_attestation_records() {
    use sha2::{Digest as _, Sha256};

    let tmp = tempfile::tempdir().unwrap();
    let store = RegistryStore::new(tmp.path()).await.unwrap();
    store
        .set_trusted_publisher("pypi:flask-test-pkg", "acme/widgets")
        .await
        .unwrap();
    store
        .set_trusted_publisher("oci:acme/models/worker", "acme/widgets")
        .await
        .unwrap();
    let key = KeyPair::generate().unwrap();
    let bytes = b"wheel bytes";
    let digest = hex::encode(Sha256::digest(bytes));
    let registry_digest = format!("sha256:{digest}");
    store.put_blob(&registry_digest, bytes).await.unwrap();
    let file = mgc_registry_server::model::PypiFile {
        name: "Flask_Test.pkg".into(),
        version: "1.0.0".into(),
        filename: "flask_test_pkg-1.0.0.whl".into(),
        digest: registry_digest.clone(),
        size: bytes.len() as i64,
        requires_python: None,
    };
    store
        .put_trusted_pypi_file(
            &file,
            &claims(),
            &key,
            1,
            &digest,
            serde_json::json!({"public": "pypi"}),
        )
        .await
        .unwrap();
    let pypi_entries = store
        .trusted_attestations("pypi:flask-test-pkg")
        .await
        .unwrap();
    assert_eq!(pypi_entries.len(), 1);
    assert_eq!(
        pypi_entries[0].sigstore_bundle,
        Some(serde_json::json!({"public": "pypi"}))
    );

    let manifest = br#"{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{"mediaType":"t","digest":"sha256:deadbeef","size":1},"layers":[]}"#;
    let manifest_sha256 = hex::encode(Sha256::digest(manifest));
    let manifest_digest = format!("sha256:{manifest_sha256}");
    store
        .put_trusted_oci_manifest(
            "oci:acme/models/worker",
            "acme/models/worker",
            "latest",
            manifest,
            &manifest_digest,
            &manifest_sha256,
            &claims(),
            &key,
            1,
            serde_json::json!({"public": "oci"}),
        )
        .await
        .unwrap();
    assert!(
        store
            .get_oci_manifest("acme/models/worker", "latest")
            .await
            .unwrap()
            .is_some()
    );
    let oci_entries = store
        .trusted_attestations("oci:acme/models/worker")
        .await
        .unwrap();
    assert_eq!(oci_entries.len(), 1);
    assert_eq!(
        oci_entries[0].payload.artifact_sha256.as_deref(),
        Some(manifest_sha256.as_str())
    );

    let rejected = mgc_registry_server::model::PypiFile {
        filename: "not-published.whl".into(),
        ..file
    };
    assert!(
        store
            .put_trusted_pypi_file(
                &rejected,
                &claims(),
                &key,
                99,
                &digest,
                serde_json::json!({"public": "rejected"}),
            )
            .await
            .is_err()
    );
    assert_eq!(
        store.get_pypi_files("Flask_Test.pkg").await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn provenance_signing_key_is_private_and_persistent() {
    let tmp = tempfile::tempdir().unwrap();
    let first = trusted::load_or_create_signing_key(tmp.path()).unwrap();
    let second = trusted::load_or_create_signing_key(tmp.path()).unwrap();
    assert_eq!(first.key_id, second.key_id);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(tmp.path().join("registry-attestation-key.json")).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }
}

#[tokio::test]
async fn trusted_routes_fail_closed_when_oidc_is_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = Arc::new(AuthService::new(Some("admin".into()), store.clone()));
    let state: AppState = (store, auth);
    let app: Router = trusted::routes().with_state(state);
    let response = app
        .oneshot(
            Request::post("/-/v1/trusted-publish/token")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"protocol":"npm","package":"@acme/widgets","oidc_token":"unused","sigstore_oidc_token":"unused"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn binding_route_requires_admin_and_signature_route_returns_chain() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let key = KeyPair::generate().unwrap();
    let auth = Arc::new(
        AuthService::new(Some("admin".into()), store.clone())
            .with_trusted_publishing(
                Some(TrustedPublishingConfig {
                    issuers: vec!["https://token.actions.githubusercontent.com".into()],
                    audience: "https://registry.example".into(),
                }),
                Some(key.clone()),
            )
            .allow_insecure_trusted_http(true),
    );
    let state: AppState = (store.clone(), auth);
    let app: Router = trusted::routes().with_state(state);

    let unauthorized = app
        .clone()
        .oneshot(
            Request::put("/-/v1/trusted-publish/bindings")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"package":"pkg","repository":"acme/pkg"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), axum::http::StatusCode::FORBIDDEN);

    let insecure_admin = app
        .clone()
        .oneshot(
            Request::put("/-/v1/trusted-publish/bindings")
                .header("authorization", "Bearer admin")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"package":"pkg","repository":"acme/pkg"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(insecure_admin.status(), axum::http::StatusCode::FORBIDDEN);
    assert_eq!(store.trusted_publisher("pkg").await.unwrap(), None);

    let bound = app
        .clone()
        .oneshot(
            Request::put("/-/v1/trusted-publish/bindings")
                .header("authorization", "Bearer admin")
                .header("content-type", "application/json")
                .extension(axum::extract::ConnectInfo(
                    "127.0.0.1:4315".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::from(r#"{"package":"pkg","repository":"acme/pkg"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bound.status(), axum::http::StatusCode::NO_CONTENT);
    assert_eq!(
        store.trusted_publisher("pkg").await.unwrap(),
        Some("acme/pkg".into())
    );

    let mut publisher_claims = claims();
    publisher_claims.repository = Some("acme/pkg".into());
    publisher_claims.sub = "repo:acme/pkg:ref:refs/heads/main".into();
    let pkg = package("pkg", "1.0.0", &valid_digest());
    let attestations = public_attestations(&pkg);
    store
        .put_trusted_package(&pkg, &publisher_claims, &key, 1, &attestations)
        .await
        .unwrap();
    let signatures_unauthorized = app
        .clone()
        .oneshot(
            Request::get("/-/v1/trusted-publish/signatures?package=pkg")
                .extension(axum::extract::ConnectInfo(
                    "127.0.0.1:4315".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        signatures_unauthorized.status(),
        axum::http::StatusCode::UNAUTHORIZED
    );

    let bad_limit = app
        .clone()
        .oneshot(
            Request::get("/-/v1/trusted-publish/signatures?package=pkg&limit=0")
                .header("authorization", "Bearer admin")
                .extension(axum::extract::ConnectInfo(
                    "127.0.0.1:4315".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_limit.status(), axum::http::StatusCode::BAD_REQUEST);

    let response = app
        .clone()
        .oneshot(
            Request::get("/-/v1/trusted-publish/signatures?package=pkg")
                .header("authorization", "Bearer admin")
                .extension(axum::extract::ConnectInfo(
                    "127.0.0.1:4315".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let bundle: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(bundle["head_sequence"], 1);
    assert_eq!(bundle["entries"].as_array().unwrap().len(), 1);

    let remote_http = app
        .oneshot(
            Request::get("/-/v1/trusted-publish/signatures?package=pkg")
                .header("authorization", "Bearer admin")
                .extension(axum::extract::ConnectInfo(
                    "203.0.113.9:4315".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(remote_http.status(), axum::http::StatusCode::FORBIDDEN);
}
