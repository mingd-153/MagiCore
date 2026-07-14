use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use chrono::Utc;
use serde_json::{json, Value};

use crate::config::Config;

pub fn router() -> Router<Arc<Config>> {
    Router::new().route("/health", get(health_handler))
}

async fn health_handler(State(cfg): State<Arc<Config>>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": cfg.name,
        "timestamp": Utc::now().to_rfc3339()
    }))
}
