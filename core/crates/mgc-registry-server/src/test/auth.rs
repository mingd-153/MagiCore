#![allow(clippy::unwrap_used)]

use super::*;
use http::HeaderValue;
use std::net::SocketAddr;

fn oidc_claims() -> OidcClaims {
    OidcClaims {
        iss: "https://token.actions.githubusercontent.com".into(),
        sub: "repo:acme/pkg:ref:refs/heads/main".into(),
        aud: mgc_oidc::claims::Audience::Single("registry".into()),
        exp: 1_900_000_000,
        iat: 1_800_000_000,
        repository: Some("acme/pkg".into()),
        job_workflow_ref: None,
        workflow_ref: None,
        event_name: Some("push".into()),
    }
}

#[tokio::test]
async fn ephemeral_token_is_package_scoped_npm_only_and_cannot_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    store
        .set_trusted_publisher("pkg", "acme/pkg")
        .await
        .unwrap();
    let auth =
        AuthService::new(Some("admin".into()), store.clone()).allow_insecure_trusted_http(true);
    auth.trusted_tokens_guard().insert(
        "short-token".into(),
        TrustedToken {
            user: User {
                name: "oidc:acme/pkg".into(),
                is_admin: false,
                role: UserRole::Publisher,
                scopes: vec!["pkg".into()],
                password: None,
                email: None,
            },
            package: "pkg".into(),
            identity: oidc_claims(),
            sigstore_oidc_token: "sigstore-jwt".into(),
            binding_generation: 1,
            expires_at: Instant::now() + Duration::from_secs(30),
        },
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer short-token"),
    );

    let loopback: SocketAddr = "127.0.0.1:4315".parse().expect("loopback peer");
    assert!(
        auth.authorize_npm_write_from(&headers, "pkg", Some(loopback))
            .await
            .is_ok()
    );
    assert_eq!(
        auth.authorize_package_write_from(&headers, "pkg", Some(loopback))
            .await
            .unwrap()
            .unwrap()
            .2,
        "sigstore-jwt"
    );
    assert_eq!(
        auth.authorize_npm_write(&headers, "another-pkg").await,
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(auth.authorize_package_read(&headers, "pkg"), Ok(()));
    assert_eq!(
        auth.authorize_package_read(&headers, "another-pkg"),
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        auth.authorize_global_read(&headers),
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        auth.authorize_write(&headers, "pkg").await,
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        auth.authorize_npm_delete(&headers, "pkg").await,
        Err(StatusCode::FORBIDDEN)
    );

    store
        .set_trusted_publisher("pkg", "other/repository")
        .await
        .unwrap();
    assert_eq!(
        auth.authorize_npm_write(&headers, "pkg").await,
        Err(StatusCode::FORBIDDEN)
    );

    store
        .set_trusted_publisher("pkg", "acme/pkg")
        .await
        .unwrap();
    assert_eq!(
        auth.authorize_npm_write(&headers, "pkg").await,
        Err(StatusCode::FORBIDDEN),
        "rebinding back to the original repository must not revive an issued token"
    );
}

#[tokio::test]
async fn docker_basic_credentials_are_scoped_to_trusted_oci_tokens() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    store
        .set_trusted_publisher("oci:acme/models", "acme/pkg")
        .await
        .unwrap();
    let auth = AuthService::new(Some("admin".into()), store).allow_insecure_trusted_http(true);
    auth.trusted_tokens_guard().insert(
        "short-token".into(),
        TrustedToken {
            user: User {
                name: "oidc:acme/pkg".into(),
                is_admin: false,
                role: UserRole::Publisher,
                scopes: vec!["oci:acme/models".into()],
                password: None,
                email: None,
            },
            package: "oci:acme/models".into(),
            identity: oidc_claims(),
            sigstore_oidc_token: "sigstore-jwt".into(),
            binding_generation: 1,
            expires_at: Instant::now() + Duration::from_secs(30),
        },
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode("mgc-oidc:short-token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Basic {encoded}")).unwrap(),
    );

    assert!(auth.authenticate_headers(&headers).is_some());
    assert!(
        auth.authorize_package_write_from(
            &headers,
            "oci:acme/models",
            Some("127.0.0.1:4315".parse().expect("loopback peer")),
        )
        .await
        .unwrap()
        .is_some()
    );
    assert_eq!(
        auth.authorize_package_write_from(
            &headers,
            "oci:acme/other",
            Some("127.0.0.1:4315".parse().expect("loopback peer")),
        )
        .await,
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        auth.authorize_write(&headers, "oci:acme/models").await,
        Err(StatusCode::FORBIDDEN)
    );
}

#[tokio::test]
async fn trusted_exchange_requires_verified_https_or_loopback_server_binding() {
    let headers = HeaderMap::new();
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let remote = AuthService::new(None, store.clone());
    let remote_peer: SocketAddr = "203.0.113.9:4315".parse().unwrap();
    assert!(!remote.trusted_transport_allowed(&headers, Some(remote_peer)));
    assert!(!remote.trusted_transport_allowed(&headers, None));

    let mut forwarded = HeaderMap::new();
    forwarded.insert("x-forwarded-proto", HeaderValue::from_static("https"));
    assert!(!remote.trusted_transport_allowed(&forwarded, Some(remote_peer)));
    assert!(
        AuthService::new(None, store.clone())
            .trusted_proxy_ips(vec!["203.0.113.9".parse().unwrap()])
            .trusted_transport_allowed(&forwarded, Some(remote_peer))
    );
    assert!(
        !AuthService::new(None, store.clone())
            .trusted_proxy_ips(vec!["203.0.113.10".parse().unwrap()])
            .trusted_transport_allowed(&forwarded, Some(remote_peer))
    );
    let loopback: SocketAddr = "127.0.0.1:4315".parse().unwrap();
    assert!(
        AuthService::new(None, store)
            .allow_insecure_trusted_http(true)
            .trusted_transport_allowed(&headers, Some(loopback))
    );
    assert!(!remote.trusted_transport_allowed(&forwarded, Some(loopback)));
}

#[tokio::test]
async fn trusted_credentials_are_rejected_on_unverified_http_for_every_route() {
    use axum::{Extension, Router, body::Body, middleware::from_fn, routing::get};
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let proxy_ip = "192.0.2.4".parse().unwrap();
    let auth = Arc::new(AuthService::new(None, store).trusted_proxy_ips(vec![proxy_ip]));
    auth.trusted_tokens_guard().insert(
        "short-token".into(),
        TrustedToken {
            user: User {
                name: "oidc:acme/pkg".into(),
                is_admin: false,
                role: UserRole::Publisher,
                scopes: vec!["pkg".into()],
                password: None,
                email: None,
            },
            package: "pkg".into(),
            identity: oidc_claims(),
            sigstore_oidc_token: "sigstore-jwt".into(),
            binding_generation: 1,
            expires_at: Instant::now() + Duration::from_secs(30),
        },
    );
    let app = Router::new()
        .route("/", get(|| async { StatusCode::OK }))
        .route_layer(from_fn(trusted_transport_middleware))
        .layer(Extension(auth));

    let denied = app
        .clone()
        .oneshot(
            axum::http::Request::get("/")
                .header("authorization", "Bearer short-token")
                .header("x-forwarded-proto", "https")
                .extension(ConnectInfo(
                    "192.0.2.9:4315"
                        .parse::<SocketAddr>()
                        .expect("untrusted peer"),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let allowed = app
        .oneshot(
            axum::http::Request::get("/")
                .header("authorization", "Bearer short-token")
                .header("x-forwarded-proto", "https")
                .extension(ConnectInfo(
                    "192.0.2.4:4315"
                        .parse::<SocketAddr>()
                        .expect("allowlisted proxy peer"),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
}

#[tokio::test]
async fn trusted_token_reads_stay_package_scoped_on_registry_routes() {
    use axum::{Extension, body::Body, middleware::from_fn};
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    for name in ["pkg", "other"] {
        let package: crate::model::Package = serde_json::from_value(serde_json::json!({
            "name": name,
            "versions": {}
        }))
        .unwrap();
        store.put_package(&package).await.unwrap();
    }
    let auth = Arc::new(AuthService::new(None, store.clone()).allow_insecure_trusted_http(true));
    auth.trusted_tokens_guard().insert(
        "short-token".into(),
        TrustedToken {
            user: User {
                name: "oidc:acme/pkg".into(),
                is_admin: false,
                role: UserRole::Publisher,
                scopes: vec!["pkg".into()],
                password: None,
                email: None,
            },
            package: "pkg".into(),
            identity: oidc_claims(),
            sigstore_oidc_token: "sigstore-jwt".into(),
            binding_generation: 1,
            expires_at: Instant::now() + Duration::from_secs(30),
        },
    );
    let app = crate::npm::routes()
        .route_layer(from_fn(trusted_transport_middleware))
        .layer(Extension(auth.clone()))
        .with_state((store, auth));
    let loopback = "127.0.0.1:4315"
        .parse::<SocketAddr>()
        .expect("loopback peer");

    let own = app
        .clone()
        .oneshot(
            axum::http::Request::get("/pkg")
                .header("authorization", "Bearer short-token")
                .extension(ConnectInfo(loopback))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(own.status(), StatusCode::OK);

    let other = app
        .clone()
        .oneshot(
            axum::http::Request::get("/other")
                .header("authorization", "Bearer short-token")
                .extension(ConnectInfo(loopback))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(other.status(), StatusCode::FORBIDDEN);

    let search = app
        .oneshot(
            axum::http::Request::get("/-/v1/search?q=pkg")
                .header("authorization", "Bearer short-token")
                .extension(ConnectInfo(loopback))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(search.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn legacy_token_cannot_publish_a_trusted_package() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    store
        .set_trusted_publisher("pkg", "acme/pkg")
        .await
        .unwrap();
    let auth = AuthService::new(Some("admin".into()), store.clone());
    auth.register_user(
        "legacy-token".into(),
        User {
            name: "publisher".into(),
            is_admin: false,
            role: UserRole::Publisher,
            scopes: vec!["pkg".into()],
            password: None,
            email: None,
        },
    )
    .await
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer legacy-token"),
    );

    assert_eq!(
        auth.authorize_npm_write_from(&headers, "pkg", Some("127.0.0.1:4315".parse().unwrap()),)
            .await,
        Err(StatusCode::FORBIDDEN)
    );
}

#[tokio::test]
async fn npm_authorization_rejects_a_foreign_protocol_namespace() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    store
        .set_trusted_publisher("pypi:foo", "acme/pkg")
        .await
        .unwrap();
    let auth = AuthService::new(Some("admin".into()), store);
    auth.trusted_tokens_guard().insert(
        "pypi-token".into(),
        TrustedToken {
            user: User {
                name: "oidc:acme/pkg".into(),
                is_admin: false,
                role: UserRole::Publisher,
                scopes: vec!["pypi:foo".into()],
                password: None,
                email: None,
            },
            package: "pypi:foo".into(),
            identity: oidc_claims(),
            sigstore_oidc_token: "sigstore-jwt".into(),
            binding_generation: 1,
            expires_at: Instant::now() + Duration::from_secs(30),
        },
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer pypi-token"),
    );

    assert_eq!(
        auth.authorize_npm_write(&headers, "pypi:foo").await,
        Err(StatusCode::BAD_REQUEST)
    );
}

#[tokio::test]
async fn expired_ephemeral_token_is_removed_during_verification() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = AuthService::new(Some("admin".into()), store);
    auth.trusted_tokens_guard().insert(
        "expired-token".into(),
        TrustedToken {
            user: User {
                name: "oidc:acme/pkg".into(),
                is_admin: false,
                role: UserRole::Publisher,
                scopes: vec!["pkg".into()],
                password: None,
                email: None,
            },
            package: "pkg".into(),
            identity: oidc_claims(),
            sigstore_oidc_token: "sigstore-jwt".into(),
            binding_generation: 1,
            expires_at: Instant::now() - Duration::from_secs(1),
        },
    );
    assert!(auth.verify_token("expired-token").is_none());
    assert!(!auth.trusted_tokens_guard().contains_key("expired-token"));
}

#[tokio::test]
async fn scoped_accounts_are_limited_to_package_reads_and_global_listing() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = AuthService::new(Some("admin-token".into()), store);
    auth.users_guard().insert(
        "scoped-token".into(),
        User {
            name: "scoped-user".into(),
            is_admin: false,
            role: UserRole::Viewer,
            scopes: vec!["pkg".into()],
            password: None,
            email: None,
        },
    );

    let mut scoped_headers = HeaderMap::new();
    scoped_headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer scoped-token"),
    );
    assert_eq!(auth.authorize_package_read(&scoped_headers, "pkg"), Ok(()));
    assert_eq!(
        auth.authorize_package_read(&scoped_headers, "other"),
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        auth.authorize_global_read(&scoped_headers),
        Err(StatusCode::FORBIDDEN)
    );

    let mut admin_headers = HeaderMap::new();
    admin_headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer admin-token"),
    );
    assert_eq!(auth.authorize_package_read(&admin_headers, "other"), Ok(()));
    assert_eq!(auth.authorize_global_read(&admin_headers), Ok(()));
}

#[tokio::test]
async fn open_registry_keeps_anonymous_reads_enabled() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = AuthService::new(None, store);

    assert_eq!(
        auth.authorize_package_read(&HeaderMap::new(), "pkg"),
        Ok(())
    );
    assert_eq!(auth.authorize_global_read(&HeaderMap::new()), Ok(()));
}

#[tokio::test]
async fn configured_account_auth_keeps_anonymous_reads_public() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let auth = AuthService::new(Some("admin-token".into()), store);

    assert_eq!(
        auth.authorize_package_read(&HeaderMap::new(), "pkg"),
        Ok(())
    );
    assert_eq!(auth.authorize_global_read(&HeaderMap::new()), Ok(()));
}

#[tokio::test]
async fn scoped_account_routes_enforce_package_and_collection_read_scope() {
    use axum::body::Body;
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(RegistryStore::new(tmp.path()).await.unwrap());
    let package = serde_json::from_value::<crate::model::Package>(serde_json::json!({
        "name": "pkg",
        "versions": {
            "1.0.0": {
                "name": "pkg",
                "version": "1.0.0",
                "dist": {
                    "integrity": "sha512-test",
                    "shasum": "test",
                    "tarball": "http://registry/pkg/-/pkg-1.0.0.tgz"
                }
            }
        },
        "dist-tags": {"latest": "1.0.0"}
    }))
    .unwrap();
    store.put_package(&package).await.unwrap();
    let auth = Arc::new(AuthService::new(Some("admin-token".into()), store.clone()));
    auth.users_guard().insert(
        "scoped-token".into(),
        User {
            name: "scoped-user".into(),
            is_admin: false,
            role: UserRole::Viewer,
            scopes: vec!["pkg".into()],
            password: None,
            email: None,
        },
    );
    let app = crate::npm::routes()
        .merge(crate::oci::routes())
        .with_state((store, auth));

    let own = app
        .clone()
        .oneshot(
            axum::http::Request::get("/pkg")
                .header("authorization", "Bearer scoped-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(own.status(), StatusCode::OK);

    let other = app
        .clone()
        .oneshot(
            axum::http::Request::get("/other")
                .header("authorization", "Bearer scoped-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(other.status(), StatusCode::FORBIDDEN);

    let search = app
        .clone()
        .oneshot(
            axum::http::Request::get("/-/v1/search?q=pkg")
                .header("authorization", "Bearer scoped-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(search.status(), StatusCode::FORBIDDEN);

    let catalog = app
        .oneshot(
            axum::http::Request::get("/v2/_catalog")
                .header("authorization", "Bearer scoped-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(catalog.status(), StatusCode::FORBIDDEN);
}
