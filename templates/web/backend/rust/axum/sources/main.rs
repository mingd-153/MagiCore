mod config;
mod routes;
mod services;

use std::sync::Arc;

use axum::{routing::get, Json, Router};
use serde_json::json;

use config::Config;

#[tokio::main]
async fn main() {
    let cfg = Arc::new(Config::load());

    let app = Router::new()
        .route("/health", get(routes::health::health_handler))
        .route("/", get(root_handler))
        .with_state(cfg.clone());

    let addr = format!("0.0.0.0:{}", cfg.port);
    println!("Starting {} (axum) on {}", cfg.name, addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root_handler(cfg: axum::extract::State<Arc<Config>>) -> Json<serde_json::Value> {
    Json(json!({
        "service": cfg.name,
        "framework": cfg.framework,
        "message": "{{project_name}} backend scaffold ready"
    }))
}
