use std::time::Instant;

use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

pub async fn request_id(mut req: Request, next: Next) -> Response {
    let id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    req.extensions_mut().insert(id.clone());

    let mut res = next.run(req).await;
    res.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&id).unwrap(),
    );
    res
}

pub async fn logger(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();
    let res = next.run(req).await;
    let latency = start.elapsed();
    let request_id = res
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    tracing::info!(
        method = %method,
        path = %uri.path(),
        status = res.status().as_u16(),
        latency_ms = latency.as_millis(),
        request_id = %request_id,
        "request"
    );
    res
}
