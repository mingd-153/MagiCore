use std::time::Instant;

use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    Error,
    HttpMessage,
};
use uuid::Uuid;

pub async fn request_id<B>(
    req: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<B>, Error>
where
    B: MessageBody + 'static,
{
    let id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    req.extensions_mut().insert(id.clone());

    let mut res = next.call(req).await?;
    res.headers_mut().insert(
        actix_web::http::header::HeaderName::from_static("x-request-id"),
        actix_web::http::header::HeaderValue::from_str(&id).unwrap(),
    );
    Ok(res)
}

pub async fn logger<B>(
    req: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<B>, Error>
where
    B: MessageBody + 'static,
{
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();
    let res = next.call(req).await?;
    let latency = start.elapsed();
    let request_id = res
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();
    tracing::info!(
        method = %method,
        path = %uri.path(),
        status = res.status().as_u16(),
        latency_ms = latency.as_millis(),
        request_id = %request_id,
        "request"
    );
    Ok(res)
}
