//! OCI Distribution Spec endpoints (/v2/*)
//! (OCI API: blob upload/download, manifest push/pull)
//! NOTE: repo names may contain slashes (e.g. `ai/mymodel`), so routes are
//! matched via `/v2/*rest` and split manually — `:name` matches one segment only.

use crate::{model::*, storage::RegistryStore, AppState};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

fn header_value(value: &str) -> Result<HeaderValue, StatusCode> {
    HeaderValue::from_str(value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// OCI routes
pub fn routes() -> Router<AppState> {
    Router::new()
        // Blob + manifest operations, dispatcher parses `rest`
        .route(
            "/v2/*rest",
            get(v2_dispatch)
                .head(v2_dispatch)
                .post(v2_dispatch)
                .put(v2_dispatch)
                .patch(v2_dispatch)
                .delete(v2_dispatch),
        )
        // Root ping + catalog (static, preferred over the catch-all)
        .route("/v2/", get(v2_root))
        .route("/v2/_catalog", get(catalog))
}

/// Split `/v2/{rest}` into `(name, section, tail)`.
/// tail excludes the section, e.g. `/blobs/uploads/` -> tail `"/"`.
fn split_oci(rest: &str) -> Option<(&str, &str, &str)> {
    for section in ["blobs/uploads", "blobs", "manifests", "tags/list"] {
        if let Some(idx) = rest.find(section) {
            let name = rest[..idx].trim_end_matches('/');
            let tail = &rest[idx + section.len()..];
            if !name.is_empty() {
                return Some((name, section, tail));
            }
        }
    }
    None
}

/// Entry point for `/v2/*rest` — dispatches by section + HTTP method.
async fn v2_dispatch(
    state: State<AppState>,
    method: Method,
    Path(rest): Path<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let Some((name, section, tail)) = split_oci(&rest) else {
        // Not a known OCI route shape
        return Err(StatusCode::NOT_FOUND);
    };
    let (store, _) = state.0;

    match section {
        "blobs/uploads" => match tail {
            "/" | "" => blob_upload_start(&store, name, &query)
                .await
                .map(|r| r.into_response()),
            uuid_tail => match method {
                Method::PATCH => {
                    blob_upload_chunk(&store, name, &uuid_tail[1..], &headers, body).await
                }
                Method::PUT => {
                    blob_upload_complete(&store, name, &uuid_tail[1..], &query, body).await
                }
                _ => Err(StatusCode::METHOD_NOT_ALLOWED),
            },
        },
        "blobs" => match method {
            Method::HEAD => blob_head(&store, name, &tail[1..]).await,
            Method::GET => blob_get(&store, name, &tail[1..]).await,
            Method::DELETE => blob_delete(&store, name, &tail[1..]).await,
            _ => Err(StatusCode::METHOD_NOT_ALLOWED),
        },
        "manifests" => match method {
            Method::GET => manifest_get(&store, name, &tail[1..]).await,
            Method::PUT => manifest_put(&store, name, &tail[1..], body).await,
            Method::DELETE => manifest_delete(&store, name, &tail[1..]).await,
            _ => Err(StatusCode::METHOD_NOT_ALLOWED),
        },
        "tags/list" if method == Method::GET => tags_list(&store, name).await,
        _ => Err(StatusCode::METHOD_NOT_ALLOWED),
    }
}

// === Root ===

async fn v2_root() -> Json<serde_json::Value> {
    Json(serde_json::json!({}))
}

// === Blob operations ===

async fn blob_head(
    store: &RegistryStore,
    name: &str,
    digest: &str,
) -> Result<Response, StatusCode> {
    match store.oci_blob_exists(name, digest).await {
        Ok(true) => Ok(StatusCode::OK.into_response()),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn blob_get(store: &RegistryStore, name: &str, digest: &str) -> Result<Response, StatusCode> {
    match store.get_oci_blob(name, digest).await {
        Ok(Some(data)) => {
            let mut resp = Response::new(axum::body::Body::from(data));
            resp.headers_mut().insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            // Add Docker-Content-Digest header
            let digest_header = format!(
                "sha256:{}",
                digest.strip_prefix("sha256:").unwrap_or(digest)
            );
            resp.headers_mut()
                .insert("docker-content-digest", header_value(&digest_header)?);
            Ok(resp.into_response())
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn blob_delete(
    store: &RegistryStore,
    name: &str,
    digest: &str,
) -> Result<Response, StatusCode> {
    if store
        .delete_oci_blob(name, digest)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// === Blob upload ===

async fn blob_upload_start(
    store: &RegistryStore,
    name: &str,
    query: &HashMap<String, String>,
) -> Result<Response, StatusCode> {
    // Cross-repo mount: POST /v2/{name}/blobs/uploads/?mount={digest}&from={repo}
    if let (Some(digest), Some(from)) = (query.get("mount"), query.get("from")) {
        if store
            .mount_oci_blob(from, digest, name)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        {
            let mut resp = Response::new(axum::body::Body::from("{}"));
            *resp.status_mut() = StatusCode::CREATED;
            resp.headers_mut()
                .insert("docker-content-digest", header_value(digest)?);
            resp.headers_mut().insert(
                axum::http::header::LOCATION,
                header_value(&format!("/v2/{}/blobs/{}", name, digest))?,
            );
            return Ok(resp);
        }
    }

    let uuid = uuid::Uuid::new_v4().to_string();
    store
        .create_oci_upload(name, &uuid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Return upload URL (JSON body + Location header — OCI spec yêu cầu header)
    let upload_url = format!("/v2/{}/blobs/uploads/{}", name, uuid);

    let mut resp = Json(OciBlobUploadResponse {
        location: upload_url.clone(),
        range: Some("0-0".to_string()),
        docker_upload_uuid: uuid,
    })
    .into_response();
    resp.headers_mut()
        .insert(axum::http::header::LOCATION, header_value(&upload_url)?);
    Ok(resp)
}

async fn blob_upload_chunk(
    store: &RegistryStore,
    name: &str,
    uuid: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    // Content-Range: bytes <start>-<end>/* — xác định vị trí append
    let start = headers
        .get("content-range")
        .and_then(|h| h.to_str().ok())
        .and_then(|r| r.strip_prefix("bytes "))
        .and_then(|r| r.split('-').next())
        .and_then(|s| s.parse::<i64>().ok());

    let offset = store
        .append_oci_upload(name, uuid, &body)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Chunk trước phải khớp offset hiện tại (resume-safe) — ponytail: bỏ qua nếu client lệch, client tự retry
    if let Some(start) = start {
        if start != offset - body.len() as i64 {
            return Err(StatusCode::RANGE_NOT_SATISFIABLE);
        }
    }

    Ok(Json(OciBlobUploadResponse {
        location: format!("/v2/{}/blobs/uploads/{}", name, uuid),
        range: Some(format!("0-{}", offset)),
        docker_upload_uuid: uuid.to_string(),
    })
    .into_response())
}

async fn blob_upload_complete(
    store: &RegistryStore,
    name: &str,
    uuid: &str,
    query: &HashMap<String, String>,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let expected_digest = query.get("digest").ok_or(StatusCode::BAD_REQUEST)?;

    // Lấy data đã upload qua session (chunked) hoặc body (single-shot)
    let data: Vec<u8> = match store.oci_upload_path(name, uuid).await {
        Ok(Some(path)) => {
            let size = std::fs::metadata(&path)
                .map_err(|_| StatusCode::NOT_FOUND)?
                .len() as usize;
            let mut buf = vec![0u8; size];
            let file = std::fs::File::open(&path).map_err(|_| StatusCode::NOT_FOUND)?;
            use std::io::Read;
            let mut file = file;
            let mut read = 0;
            while read < size {
                let n = file
                    .read(&mut buf[read..])
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                if n == 0 {
                    break;
                }
                read += n;
            }
            buf.truncate(read);
            buf
        }
        _ => {
            if body.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            body.to_vec()
        }
    };

    // Verify body digest
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let computed = format!("sha256:{}", hex::encode(hasher.finalize()));

    if computed != *expected_digest {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Store blob
    store
        .put_oci_blob(name, &computed, &data)
        .await
        .map_err(|e| {
            tracing::error!("put_oci_blob {name} {computed}: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    store.finish_oci_upload(name, uuid).await.map_err(|e| {
        tracing::error!("finish_oci_upload: {e:#}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut resp = Response::new(axum::body::Body::from("{}"));
    *resp.status_mut() = StatusCode::CREATED;
    resp.headers_mut()
        .insert("docker-content-digest", header_value(&computed)?);
    resp.headers_mut().insert(
        axum::http::header::LOCATION,
        header_value(&format!("/v2/{}/blobs/{}", name, computed))?,
    );
    Ok(resp)
}

// === Manifest operations ===

async fn manifest_get(
    store: &RegistryStore,
    name: &str,
    reference: &str,
) -> Result<Response, StatusCode> {
    match store.get_oci_manifest_raw(name, reference).await {
        Ok(Some((manifest_json, digest))) => {
            let mut resp = Response::new(axum::body::Body::from(manifest_json));
            resp.headers_mut().insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/vnd.oci.image.manifest.v1+json"),
            );
            // Add Docker-Content-Digest (stored digest of the exact bytes pushed)
            resp.headers_mut()
                .insert("docker-content-digest", header_value(&digest)?);
            Ok(resp.into_response())
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn manifest_put(
    store: &RegistryStore,
    name: &str,
    reference: &str,
    body: Bytes,
) -> Result<Response, StatusCode> {
    // Validate JSON early; digest is computed from the exact bytes pushed
    if serde_json::from_slice::<serde_json::Value>(&body).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }

    store
        .put_oci_manifest(name, reference, &body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::CREATED.into_response())
}

async fn manifest_delete(
    store: &RegistryStore,
    name: &str,
    reference: &str,
) -> Result<Response, StatusCode> {
    store
        .delete_oci_manifest(name, reference)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

// === Tags listing ===

async fn tags_list(store: &RegistryStore, name: &str) -> Result<Response, StatusCode> {
    let tags = store
        .list_oci_tags(name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({
        "name": name,
        "tags": tags
    }))
    .into_response())
}

// === Catalog ===

async fn catalog(
    State((store, _)): State<AppState>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repos = store
        .list_oci_repos()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({
        "repositories": repos
    })))
}
