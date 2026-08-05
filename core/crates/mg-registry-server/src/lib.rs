//! mg-registry-server — private registry server (MegaGate)
//! /npm + /v2 endpoints, private-only (404 for public), client-side scoping.
//! (Server registry private-only — binary riêng `mg-registry`, sys-mg/12 A14)

pub mod auth;
pub mod model;
pub mod npm;
pub mod oci;
pub mod pypi;
pub mod ratelimit;
pub mod storage;

/// Application state type alias
pub type AppState = (std::sync::Arc<crate::storage::RegistryStore>, std::sync::Arc<crate::auth::AuthService>);

/// Start the registry server (used by bin `mg-registry` and `mg registry serve`)
/// rate_limit_rps: số request/giây/IP cho phép (0 = tắt)
pub async fn serve(
    host: String,
    port: u16,
    store_dir: String,
    admin_token: Option<String>,
    max_body_size: usize,
    rate_limit_rps: usize,
) -> anyhow::Result<()> {
    let store = std::sync::Arc::new(storage::RegistryStore::new(&store_dir).await?);
    let auth_service = std::sync::Arc::new(auth::AuthService::new(admin_token, store.clone()));
    // Nạp user persist từ DB (10-task-plan Phase 3)
    auth_service.load_from_db().await?;
    let limiter = std::sync::Arc::new(ratelimit::RateLimiter::new(ratelimit::RateLimitConfig {
        max_requests: rate_limit_rps,
        window_secs: 1,
    }));

    let mut app = axum::Router::new()
        .merge(npm::routes())
        .merge(oci::routes())
        .merge(pypi::routes())
        .layer(axum::extract::DefaultBodyLimit::max(max_body_size))
        .layer(tower_http::cors::CorsLayer::permissive());

    // Fail-closed: admin token cấu hình → mọi route yêu cầu Bearer/Basic hợp lệ
    if auth_service.admin_token.is_some() {
        app = app
            .route_layer(axum::middleware::from_fn(auth::auth_middleware))
            .layer(axum::Extension(auth_service.clone()));
    }

    if rate_limit_rps > 0 {
        app = app
            .route_layer(axum::middleware::from_fn(ratelimit::rate_limit_middleware))
            .layer(axum::Extension(limiter));
    }
    let app = app.with_state((store, auth_service));

    let addr: std::net::SocketAddr = format!("{}:{}", host, port).parse()?;
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}