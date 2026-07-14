use axum::{Router, routing::get, Json};
use std::net::SocketAddr;
use serde_json::json;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/api/health", get(|| async { Json(json!({"status":"ok"})) }));
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
