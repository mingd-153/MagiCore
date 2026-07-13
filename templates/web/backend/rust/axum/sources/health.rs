use std::sync::Arc;

use axum::{extract::State, Json};
use chrono::Utc;
use serde_json::{json, Value};

use crate::config::Config;

pub async fn health_handler(State(cfg): State<Arc<Config>>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": cfg.name,
        "timestamp": Utc::now().to_rfc3339()
    }))
}
