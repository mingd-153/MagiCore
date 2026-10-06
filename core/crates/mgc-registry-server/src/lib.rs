//! mgc-registry-server — private registry server (MagiCore)
//! /npm + /v2 endpoints, private-only (404 for public), client-side scoping.
//! Private registry server exposed by the separate mgc-registry binary — registry riêng tư chạy trong binary mgc-registry.

pub mod auth;
pub mod model;
pub mod npm;
pub mod oci;
pub mod pypi;
pub mod ratelimit;
pub mod storage;
pub mod trusted;

/// Application state type alias
pub type AppState = (
    std::sync::Arc<crate::storage::RegistryStore>,
    std::sync::Arc<crate::auth::AuthService>,
);

#[derive(Debug, Clone)]
pub struct RegistryServerConfig {
    pub host: String,
    pub port: u16,
    pub store_dir: String,
    pub admin_token: Option<String>,
    pub max_body_size: usize,
    pub rate_limit_rps: usize,
    pub upstream: Option<String>,
    pub storage: Option<String>,
    pub oidc_issuers: Vec<String>,
    pub oidc_audience: Option<String>,
    pub trusted_proxy_ips: Vec<std::net::IpAddr>,
}

/// Start the registry server (used by bin `mgc-registry` và `mgc registry serve`)
/// rate_limit_rps: số request/giây/IP cho phép (0 = tắt)
/// upstream: registry URL để proxy GET-miss (ITEM 4). None = private-only.
/// storage: "local" hoặc "s3://bucket/prefix" (ITEM 5).
pub async fn serve(config: RegistryServerConfig) -> anyhow::Result<()> {
    let mut store = storage::RegistryStore::new(&config.store_dir).await?;
    store.set_upstream(config.upstream);
    store.set_backend(config.storage.as_deref())?;
    let store = std::sync::Arc::new(store);
    let trusted_config = match (config.oidc_issuers, config.oidc_audience) {
        (issuers, Some(audience)) if !issuers.is_empty() && !audience.trim().is_empty() => {
            Some(trusted::TrustedPublishingConfig { issuers, audience })
        }
        _ => None,
    };
    if trusted_config.is_some()
        && config
            .admin_token
            .as_deref()
            .is_none_or(|token| token.trim().is_empty())
    {
        anyhow::bail!("trusted publishing requires a configured registry admin token");
    }
    let signing_key = if trusted_config.is_some() {
        let store_dir = std::path::Path::new(&config.store_dir);
        if !trusted::signing_key_path(store_dir).exists()
            && store.has_trusted_attestations().await?
        {
            anyhow::bail!("registry provenance signing key is missing while signed records exist");
        }
        let key = trusted::load_or_create_signing_key(store_dir)?;
        if store
            .trusted_attestation_key_id()
            .await?
            .is_some_and(|stored| stored != key.key_id)
        {
            anyhow::bail!("registry provenance signing key does not match the existing log");
        }
        Some(key)
    } else {
        None
    };
    let auth_service = std::sync::Arc::new(
        auth::AuthService::new(config.admin_token, store.clone())
            .with_trusted_publishing(trusted_config, signing_key)
            .allow_insecure_trusted_http(
                config
                    .host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback()),
            )
            .trusted_proxy_ips(config.trusted_proxy_ips),
    );
    // Nạp user persist từ DB (10-task-plan Phase 3)
    auth_service.load_from_db().await?;
    let limiter = std::sync::Arc::new(ratelimit::RateLimiter::new(ratelimit::RateLimitConfig {
        max_requests: config.rate_limit_rps,
        window_secs: 1,
    }));

    let mut app = axum::Router::new()
        .merge(npm::routes())
        .merge(trusted::routes())
        .merge(oci::routes())
        .merge(pypi::routes())
        .layer(axum::extract::DefaultBodyLimit::max(config.max_body_size))
        .layer(tower_http::cors::CorsLayer::permissive());

    app = app.route_layer(axum::middleware::from_fn(
        auth::trusted_transport_middleware,
    ));

    // Fail-closed: admin token cấu hình → mọi route yêu cầu Bearer/Basic hợp lệ
    if auth_service.admin_token.is_some() {
        app = app.route_layer(axum::middleware::from_fn(auth::auth_middleware));
    } else {
        // Loud opt-open default: without an admin token there is no auth
        // layer at all (self-registration stays public per npm convention,
        // so anyone can publish). Production deployments MUST pass
        // --admin-token; this warning is the only thing standing between
        // a forgotten flag and an open registry.
        // (Không token = mở hoàn toàn — production BẮT BUỘC --admin-token.)
        eprintln!(
            "WARNING: starting registry WITHOUT --admin-token: no authentication is enforced and anyone can publish (npm-convention open registration)"
        );
    }

    if config.rate_limit_rps > 0 {
        app = app
            .route_layer(axum::middleware::from_fn(ratelimit::rate_limit_middleware))
            .layer(axum::Extension(limiter));
    }
    // Keep auth context outside route middleware so `from_fn` extractors can read it.
    // Đặt auth context ngoài middleware route để extractor `from_fn` đọc được.
    app = app.layer(axum::Extension(auth_service.clone()));
    let app = app.with_state((store, auth_service));

    let addr: std::net::SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
