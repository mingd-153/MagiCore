use actix_web::{web, HttpResponse};
use chrono::Utc;
use serde_json::json;

use crate::config::Config;

pub async fn health_handler(cfg: web::Data<Config>) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "status": "ok",
        "service": cfg.name,
        "timestamp": Utc::now().to_rfc3339()
    }))
}
