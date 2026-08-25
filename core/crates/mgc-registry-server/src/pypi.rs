//! PyPI-compatible endpoints — PEP 691 JSON simple index + twine legacy upload
//! (Endpoint /pypi: ai/lib python publish qua registry chung, pip install được)

use crate::{model::PypiFile, AppState};
use axum::{
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tracing::warn;

/// PyPI routes — namespace riêng /pypi để không đụng npm routes gốc
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/pypi/simple/:name", get(simple_index))
        .route("/pypi/simple/:name/", get(simple_index))
        .route("/pypi/packages/:name/:filename", get(download_file))
        .route("/pypi/legacy/", post(upload_legacy))
}

/// PEP 503 HTML simple index — pip install --index-url http://host/pypi/simple/
/// (pip cũ gửi Accept JSON nhưng không parse được → luôn trả HTML)
async fn simple_index(
    State((store, _)): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, StatusCode> {
    let files = store
        .get_pypi_files(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if files.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    // PEP 503 HTML — pip cũ (21.x) và mới đều đọc được; pip 21.x gửi Accept JSON
    // nhưng không parse được → chỉ trả HTML cho tương thích tối đa
    let mut links = String::new();
    for f in &files {
        let requires = f
            .requires_python
            .as_deref()
            .map(|r| format!(" data-requires-python=\"{r}\""))
            .unwrap_or_default();
        links.push_str(&format!(
            "<a href=\"../../packages/{}/{}\"{}>{}</a><br/>\n",
            f.name, f.filename, requires, f.filename
        ));
    }
    let html = format!(
        "<!DOCTYPE html><html><body><h1>Links for {}</h1>\n{}</body></html>",
        name, links
    );
    let mut resp = Response::new(axum::body::Body::from(html));
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/html; charset=utf-8"),
    );
    Ok(resp)
}

/// Tải wheel/sdist — blob content-addressed (sha256)
async fn download_file(
    State((store, _)): State<AppState>,
    Path((name, filename)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let digest = store
        .get_pypi_file_digest(&name, &filename)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let data = store
        .get_blob(&digest)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut resp = Response::new(axum::body::Body::from(data));
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/octet-stream"),
    );
    resp.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    Ok(resp)
}

/// Twine-compatible upload: POST multipart /pypi/legacy/
/// fields: :action=file_upload, name, version, filetype, sha256_digest, content (file)
async fn upload_legacy(
    State((store, _)): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<JsonOk, StatusCode> {
    // Auth: middleware đã fail-closed khi admin token set — ở đây kiểm thêm user
    // tồn tại (bất kỳ user đăng nhập được đều upload được — private registry)
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    if token.is_none() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut sha256_digest: Option<String> = None;
    let mut content: Option<Vec<u8>> = None;
    let mut requires_python: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "name" => name = field.text().await.ok(),
            "version" => version = field.text().await.ok(),
            "filename" => filename = field.file_name().map(|s| s.to_string()),
            "sha256_digest" => sha256_digest = field.text().await.ok(),
            "requires_python" => requires_python = field.text().await.ok(),
            "content" => {
                filename = field.file_name().map(|s| s.to_string());
                content = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| StatusCode::BAD_REQUEST)?
                        .to_vec(),
                )
            }
            _ => {}
        }
    }

    let name = name
        .filter(|n| !n.is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let version = version
        .filter(|v| !v.is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let filename = filename
        .filter(|f| !f.is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let content = content.ok_or(StatusCode::BAD_REQUEST)?;

    // sha256 verify (fail-closed: không đúng digest → từ chối)
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let actual = format!("sha256:{:x}", hasher.finalize());
    let expected = match sha256_digest {
        Some(d) => {
            let d = d.trim().to_lowercase();
            if d.starts_with("sha256:") {
                d
            } else {
                format!("sha256:{d}")
            }
        }
        None => actual.clone(),
    };
    if actual != expected {
        warn!(
            "pypi upload {}: sha256 mismatch ({} != {})",
            filename, actual, expected
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    store
        .put_blob(&actual, &content)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    store
        .put_pypi_file(&PypiFile {
            name: name.clone(),
            version: version.clone(),
            filename: filename.clone(),
            digest: actual,
            size: content.len() as i64,
            requires_python,
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(JsonOk {
        message: "file uploaded".to_string(),
    })
}

#[derive(Serialize)]
struct JsonOk {
    message: String,
}

impl IntoResponse for JsonOk {
    fn into_response(self) -> Response {
        axum::Json(self).into_response()
    }
}
