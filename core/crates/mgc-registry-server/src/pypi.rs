//! PyPI-compatible endpoints — PEP 691 JSON simple index + twine legacy upload
//! (Endpoint /pypi: ai/lib python publish qua registry chung, pip install được)

use crate::{AppState, model::PypiFile};
use axum::{
    Router,
    extract::{ConnectInfo, Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
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
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Response, StatusCode> {
    let package_scope = crate::trusted::scoped_package("pypi", &name)?;
    auth.authorize_package_read(&headers, &package_scope)?;
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
            .map(|r| format!(" data-requires-python=\"{}\"", escape_html(r)))
            .unwrap_or_default();
        let package_path = encode_path_segment(&f.name);
        let filename_path = encode_path_segment(&f.filename);
        links.push_str(&format!(
            "<a href=\"/pypi/packages/{}/{}\"{}>{}</a><br/>\n",
            escape_html(&package_path),
            escape_html(&filename_path),
            requires,
            escape_html(&f.filename)
        ));
    }
    let html = format!(
        "<!DOCTYPE html><html><body><h1>Links for {}</h1>\n{}</body></html>",
        escape_html(&name),
        links
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
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    Path((name, filename)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let package_scope = crate::trusted::scoped_package("pypi", &name)?;
    auth.authorize_package_read(&headers, &package_scope)?;
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
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    mut multipart: Multipart,
) -> Result<JsonOk, StatusCode> {
    // Reject missing credentials before buffering multipart content — từ chối auth thiếu trước khi đọc file.
    if auth.admin_token.is_some() && auth.authenticate_headers(&headers).is_none() {
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
    let name = crate::trusted::canonical_pypi_name(&name)?;
    let package_scope = crate::trusted::scoped_package("pypi", &name)?;
    let trusted_publisher = auth
        .authorize_package_write_from(
            &headers,
            &package_scope,
            peer.map(|ConnectInfo(address)| address),
        )
        .await?;
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

    let file = PypiFile {
        name: name.clone(),
        version: version.clone(),
        filename: filename.clone(),
        digest: actual.clone(),
        size: content.len() as i64,
        requires_python,
    };
    if let Some((identity, binding_generation, sigstore_oidc_token)) = trusted_publisher {
        let artifact_sha256 = actual
            .strip_prefix("sha256:")
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let key = auth.attestation_key().ok_or(StatusCode::NOT_FOUND)?;
        let bundle = crate::trusted::sign_sigstore_attestation(
            &sigstore_oidc_token,
            &identity,
            &package_scope,
            &version,
            &format!("{name}/{filename}"),
            &actual,
            artifact_sha256,
        )
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        store
            .put_trusted_pypi_file(
                &file,
                &identity,
                key,
                binding_generation,
                artifact_sha256,
                bundle,
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    } else {
        store
            .put_pypi_file(&file)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(JsonOk {
        message: "file uploaded".to_string(),
    })
}

/// Escape untrusted text before placing it in an HTML node or quoted attribute.
/// Escape dữ liệu không tin cậy trước khi đưa vào node HTML hoặc thuộc tính có quote.
fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Percent-encode one UTF-8 path segment, preserving only RFC 3986 unreserved bytes.
/// Mã hóa percent một segment UTF-8, chỉ giữ nguyên byte unreserved theo RFC 3986.
fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
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

#[cfg(test)]
#[path = "test/pypi.rs"]
mod tests;
