mod config;
mod routes;
mod services;

use std::sync::Arc;
use axum::Router;

#[tokio::main]
async fn main() {
    let cfg = Arc::new(config::Config::load());
    let app = Router::new()
        .nest("/api", routes::health::router())
        .with_state(Arc::clone(&cfg));
    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", cfg.port).parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
