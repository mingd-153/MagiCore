mod config;
mod middleware;
mod routes;
mod services;

use std::sync::Arc;

use axum::Router;
use tokio::signal;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = Arc::new(config::Config::load());
    let app = Router::new()
        .nest("/api", routes::health::router())
        .layer(axum::middleware::from_fn(middleware::request_id))
        .layer(axum::middleware::from_fn(middleware::logger))
        .layer(CorsLayer::permissive())
        .with_state(Arc::clone(&cfg));

    let addr = format!("0.0.0.0:{}", cfg.port).parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    signal::ctrl_c().await.unwrap();
    tracing::info!("shutdown signal received");
}
