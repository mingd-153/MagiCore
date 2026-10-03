//! OCI Distribution Spec endpoints (/v2/*)
//! (OCI API: blob upload/download, manifest push/pull)
//! NOTE: repo names may contain slashes (e.g. `ai/mymodel`), so routes are
//! matched via `/v2/*rest` and split manually — `:name` matches one segment only.

use crate::{AppState, model::*, storage::RegistryStore};
use axum::{
    Router,
    body::Bytes,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::get,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::SocketAddr;

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
    let mut selected: Option<(usize, &str, &str, &str)> = None;
    for section in ["blobs/uploads", "blobs", "manifests", "tags/list"] {
        let marker = format!("/{section}");
        for (index, _) in rest.rmatch_indices(&marker) {
            let end = index + marker.len();
            if end < rest.len() && rest.as_bytes()[end] != b'/' {
                continue;
            }
            let name = &rest[..index];
            let tail = &rest[end..];
            if name.is_empty() || !valid_oci_route_tail(section, tail) {
                continue;
            }
            let replace = selected
                .as_ref()
                .is_none_or(|(best_index, best_section, _, _)| {
                    index > *best_index
                        || (index == *best_index && section.len() > best_section.len())
                });
            if replace {
                selected = Some((index, section, name, tail));
            }
        }
    }
    selected.map(|(_, section, name, tail)| (name, section, tail))
}

fn valid_oci_route_tail(section: &str, tail: &str) -> bool {
    let is_single_segment = |value: &str| {
        value
            .strip_prefix('/')
            .is_some_and(|segment| !segment.is_empty() && !segment.contains('/'))
    };
    match section {
        "blobs/uploads" => tail.is_empty() || tail == "/" || is_single_segment(tail),
        "blobs" | "manifests" => is_single_segment(tail),
        "tags/list" => tail.is_empty(),
        _ => false,
    }
}

/// Entry point for `/v2/*rest` — dispatches by section + HTTP method.
async fn v2_dispatch(
    state: State<AppState>,
    method: Method,
    Path(rest): Path<String>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Query(query): Query<HashMap<String, String>>,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let Some((name, section, tail)) = split_oci(&rest) else {
        // Not a known OCI route shape
        return Err(StatusCode::NOT_FOUND);
    };
    let (store, auth) = state.0;
    // Upload sessions also mutate storage — phiên upload cũng ghi dữ liệu.
    let trusted_publisher = if section == "blobs/uploads"
        || !matches!(method, Method::GET | Method::HEAD)
    {
        let package_scope = crate::trusted::scoped_package("oci", name)?;
        let publisher = auth
            .authorize_package_write_from(
                &headers,
                &package_scope,
                peer.map(|ConnectInfo(address)| address),
            )
            .await?;
        if publisher.is_some()
            && !(section == "blobs/uploads" || (section == "manifests" && method == Method::PUT))
        {
            return Err(StatusCode::FORBIDDEN);
        }
        publisher
    } else {
        let package_scope = crate::trusted::scoped_package("oci", name)?;
        auth.authorize_package_read(&headers, &package_scope)?;
        None
    };

    // Cross-repository mounts also require read access to the source repository.
    // Mount liên repository cũng cần quyền đọc repository nguồn.
    if section == "blobs/uploads"
        && query.contains_key("mount")
        && let Some(from) = query.get("from")
    {
        let source_scope = crate::trusted::scoped_package("oci", from)?;
        auth.authorize_package_read(&headers, &source_scope)?;
    }

    match section {
        "blobs/uploads" => match tail {
            "/" | "" if method == Method::POST => blob_upload_start(&store, name, &query)
                .await
                .map(|r| r.into_response()),
            "/" | "" => Err(StatusCode::METHOD_NOT_ALLOWED),
            uuid_tail => match method {
                Method::PATCH => {
                    blob_upload_chunk(&store, name, &uuid_tail[1..], &headers, body).await
                }
                Method::PUT => {
                    blob_upload_complete(&store, name, &uuid_tail[1..], &headers, &query, body)
                        .await
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
            Method::PUT => {
                manifest_put(
                    &store,
                    name,
                    &tail[1..],
                    body,
                    trusted_publisher,
                    auth.attestation_key(),
                )
                .await
            }
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
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let (Some(digest), Some(from)) = (query.get("mount"), query.get("from"))
        && store
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

    let uuid = uuid::Uuid::new_v4().to_string();
    store
        .create_oci_upload(name, &uuid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Return upload URL (JSON body + Location header — OCI spec yêu cầu header)
    let upload_url = format!("/v2/{}/blobs/uploads/{}", name, uuid);

    let mut resp = Json(OciBlobUploadResponse {
        location: upload_url.clone(),
        range: None,
        docker_upload_uuid: uuid,
    })
    .into_response();
    *resp.status_mut() = StatusCode::ACCEPTED;
    resp.headers_mut()
        .insert(axum::http::header::LOCATION, header_value(&upload_url)?);
    Ok(resp)
}

fn parse_upload_range(headers: &HeaderMap, body_len: usize) -> Result<i64, StatusCode> {
    let value = headers
        .get("content-range")
        .and_then(|header| header.to_str().ok())
        .ok_or(StatusCode::RANGE_NOT_SATISFIABLE)?;
    let (start, end) = value
        .split_once('-')
        .ok_or(StatusCode::RANGE_NOT_SATISFIABLE)?;
    if start.is_empty()
        || end.is_empty()
        || !start.bytes().all(|byte| byte.is_ascii_digit())
        || !end.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(StatusCode::RANGE_NOT_SATISFIABLE);
    }
    let start = start
        .parse::<u64>()
        .map_err(|_| StatusCode::RANGE_NOT_SATISFIABLE)?;
    let end = end
        .parse::<u64>()
        .map_err(|_| StatusCode::RANGE_NOT_SATISFIABLE)?;
    let declared_len = end
        .checked_sub(start)
        .and_then(|range| range.checked_add(1))
        .ok_or(StatusCode::RANGE_NOT_SATISFIABLE)?;
    if declared_len != body_len as u64 {
        return Err(StatusCode::RANGE_NOT_SATISFIABLE);
    }
    i64::try_from(start).map_err(|_| StatusCode::RANGE_NOT_SATISFIABLE)
}

async fn blob_upload_chunk(
    store: &RegistryStore,
    name: &str,
    uuid: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let start = parse_upload_range(headers, body.len())?;
    let offset = store
        .append_oci_upload(name, uuid, start, &body)
        .await
        .map_err(oci_append_error_status)?;

    let upload_url = format!("/v2/{}/blobs/uploads/{}", name, uuid);
    let last_byte = offset - 1;
    let mut response = Json(OciBlobUploadResponse {
        location: upload_url.clone(),
        range: Some(format!("0-{last_byte}")),
        docker_upload_uuid: uuid.to_string(),
    })
    .into_response();
    *response.status_mut() = StatusCode::ACCEPTED;
    response
        .headers_mut()
        .insert(axum::http::header::LOCATION, header_value(&upload_url)?);
    response
        .headers_mut()
        .insert("range", header_value(&format!("0-{last_byte}"))?);
    Ok(response)
}

async fn blob_upload_complete(
    store: &RegistryStore,
    name: &str,
    uuid: &str,
    headers: &HeaderMap,
    query: &HashMap<String, String>,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let expected_digest = query.get("digest").ok_or(StatusCode::BAD_REQUEST)?;
    validate_sha256_digest(expected_digest)?;
    let digest_header = header_value(expected_digest)?;
    let location_header = header_value(&format!("/v2/{name}/blobs/{expected_digest}"))?;
    if let Some(offset) = store.oci_upload_offset(name, uuid).await.map_err(|error| {
        tracing::error!("find OCI upload {name}/{uuid}: {error:#}");
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        if !body.is_empty() {
            let start = parse_upload_range(headers, body.len())?;
            if start != offset {
                return Err(StatusCode::RANGE_NOT_SATISFIABLE);
            }
            store
                .append_oci_upload(name, uuid, start, &body)
                .await
                .map_err(oci_append_error_status)?;
        }
        store
            .finalize_oci_upload(name, uuid, expected_digest)
            .await
            .map_err(|error| {
                tracing::error!("finalize_oci_upload {name}/{uuid}: {error:#}");
                StatusCode::BAD_REQUEST
            })?;
    } else {
        if body.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        if body.len() as u64 > crate::storage::max_oci_blob_bytes() {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        store
            .put_oci_blob(name, expected_digest, &body)
            .await
            .map_err(|error| {
                tracing::error!("put_oci_blob {name} {expected_digest}: {error:#}");
                StatusCode::BAD_REQUEST
            })?;
    }

    let mut resp = Response::new(axum::body::Body::from("{}"));
    *resp.status_mut() = StatusCode::CREATED;
    resp.headers_mut()
        .insert("docker-content-digest", digest_header);
    resp.headers_mut()
        .insert(axum::http::header::LOCATION, location_header);
    Ok(resp)
}

/// Map storage append failures to their OCI HTTP status.
/// Chuyển lỗi append storage sang status HTTP OCI tương ứng.
fn oci_append_error_status(error: crate::storage::OciUploadAppendError) -> StatusCode {
    match error {
        crate::storage::OciUploadAppendError::SessionNotFound => StatusCode::NOT_FOUND,
        crate::storage::OciUploadAppendError::OffsetMismatch => StatusCode::RANGE_NOT_SATISFIABLE,
        crate::storage::OciUploadAppendError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        crate::storage::OciUploadAppendError::Storage(error) => {
            tracing::error!("OCI upload chunk storage failed: {error:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

// === Manifest operations ===

async fn manifest_get(
    store: &RegistryStore,
    name: &str,
    reference: &str,
) -> Result<Response, StatusCode> {
    match store.get_oci_manifest_raw(name, reference).await {
        Ok(Some((manifest_json, digest))) => {
            let media_type = serde_json::from_str::<serde_json::Value>(&manifest_json)
                .ok()
                .and_then(|manifest| {
                    manifest
                        .get("mediaType")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut resp = Response::new(axum::body::Body::from(manifest_json));
            resp.headers_mut()
                .insert(axum::http::header::CONTENT_TYPE, header_value(&media_type)?);
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
    trusted_publisher: Option<(mgc_oidc::OidcClaims, u64, String)>,
    attestation_key: Option<&mgc_crypto::KeyPair>,
) -> Result<Response, StatusCode> {
    validate_oci_reference(reference)?;
    let manifest_digest = format!("sha256:{}", hex::encode(Sha256::digest(&body)));
    if reference.starts_with("sha256:") && reference != manifest_digest {
        return Err(StatusCode::BAD_REQUEST);
    }
    let manifest_location = format!("/v2/{name}/manifests/{reference}");
    let location_header = header_value(&manifest_location)?;
    let digest_header = header_value(&manifest_digest)?;
    validate_oci_manifest(store, name, &body).await?;

    if let Some((identity, binding_generation, sigstore_oidc_token)) = trusted_publisher {
        let package_scope = crate::trusted::scoped_package("oci", name)?;
        let artifact_sha256 = hex::encode(Sha256::digest(&body));
        let registry_digest = format!("sha256:{artifact_sha256}");
        let public_bundle = crate::trusted::sign_sigstore_attestation(
            &sigstore_oidc_token,
            &identity,
            &package_scope,
            reference,
            &format!("{name}@{reference}"),
            &registry_digest,
            &artifact_sha256,
        )
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let key = attestation_key.ok_or(StatusCode::NOT_FOUND)?;
        store
            .put_trusted_oci_manifest(
                &package_scope,
                name,
                reference,
                &body,
                &registry_digest,
                &artifact_sha256,
                &identity,
                key,
                binding_generation,
                public_bundle,
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    } else {
        store
            .put_oci_manifest(name, reference, &body)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let mut response = StatusCode::CREATED.into_response();
    response.headers_mut().insert("location", location_header);
    response
        .headers_mut()
        .insert("docker-content-digest", digest_header);
    Ok(response)
}

fn validate_sha256_digest(digest: &str) -> Result<(), StatusCode> {
    let Some(hash) = digest.strip_prefix("sha256:") else {
        return Err(StatusCode::BAD_REQUEST);
    };
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

fn validate_oci_reference(reference: &str) -> Result<(), StatusCode> {
    if reference.starts_with("sha256:") {
        return validate_sha256_digest(reference);
    }
    let valid_tag = !reference.is_empty()
        && reference.len() <= 128
        && reference
            .as_bytes()
            .first()
            .is_some_and(|first| first.is_ascii_alphanumeric() || *first == b'_')
        && reference
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte));
    if valid_tag {
        Ok(())
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

fn validate_oci_descriptor(value: &serde_json::Value) -> Result<(&str, &str, u64), StatusCode> {
    let media_type = value
        .get("mediaType")
        .and_then(serde_json::Value::as_str)
        .filter(|media_type| !media_type.is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let digest = value
        .get("digest")
        .and_then(serde_json::Value::as_str)
        .ok_or(StatusCode::BAD_REQUEST)?;
    validate_sha256_digest(digest)?;
    let size = value
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .ok_or(StatusCode::BAD_REQUEST)?;
    Ok((media_type, digest, size))
}

async fn validate_oci_manifest(
    store: &RegistryStore,
    repository: &str,
    body: &[u8],
) -> Result<(), StatusCode> {
    const MAX_INDEX_DEPTH: usize = 8;
    const OCI_IMAGE_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
    const DOCKER_IMAGE_MANIFEST: &str = "application/vnd.docker.distribution.manifest.v2+json";
    const OCI_IMAGE_INDEX: &str = "application/vnd.oci.image.index.v1+json";
    const DOCKER_MANIFEST_LIST: &str = "application/vnd.docker.distribution.manifest.list.v2+json";
    const MAX_INDEX_TREE_BYTES: u64 = 64 * 1024 * 1024;
    let supported = |media_type: &str| {
        matches!(
            media_type,
            OCI_IMAGE_MANIFEST | DOCKER_IMAGE_MANIFEST | OCI_IMAGE_INDEX | DOCKER_MANIFEST_LIST
        )
    };
    if body.len() as u64 > MAX_INDEX_TREE_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let mut pending = vec![(body.to_vec(), 0usize)];
    let mut manifest_metadata = HashMap::<String, (String, u64)>::new();
    let mut verified_blobs = HashMap::<String, u64>::new();
    let mut descriptor_budget = 10_000usize;
    let mut tree_bytes_budget = MAX_INDEX_TREE_BYTES - body.len() as u64;
    let mut verification_bytes_budget = crate::storage::max_oci_manifest_verify_bytes();
    while let Some((bytes, depth)) = pending.pop() {
        if depth > MAX_INDEX_DEPTH {
            return Err(StatusCode::BAD_REQUEST);
        }
        let manifest_size = bytes.len() as u64;
        let manifest: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
        if manifest
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            != Some(2)
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        let media_type = manifest
            .get("mediaType")
            .and_then(serde_json::Value::as_str)
            .filter(|media_type| supported(media_type))
            .ok_or(StatusCode::BAD_REQUEST)?;
        let current_digest = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
        manifest_metadata.insert(current_digest, (media_type.to_owned(), manifest_size));
        if matches!(media_type, OCI_IMAGE_INDEX | DOCKER_MANIFEST_LIST) {
            let descriptors = manifest
                .get("manifests")
                .and_then(serde_json::Value::as_array)
                .ok_or(StatusCode::BAD_REQUEST)?;
            if descriptors.len() > descriptor_budget {
                return Err(StatusCode::BAD_REQUEST);
            }
            for descriptor in descriptors {
                descriptor_budget -= 1;
                let (child_media_type, digest, expected_size) =
                    validate_oci_descriptor(descriptor)?;
                if !supported(child_media_type) {
                    return Err(StatusCode::BAD_REQUEST);
                }
                if let Some((known_media_type, known_size)) = manifest_metadata.get(digest) {
                    if known_media_type != child_media_type || *known_size != expected_size {
                        return Err(StatusCode::BAD_REQUEST);
                    }
                    continue;
                }
                let (child, stored_digest) = store
                    .get_oci_manifest_raw(repository, digest)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                    .ok_or(StatusCode::BAD_REQUEST)?;
                if child.len() as u64 != expected_size
                    || stored_digest != digest
                    || format!("sha256:{}", hex::encode(Sha256::digest(child.as_bytes()))) != digest
                {
                    return Err(StatusCode::BAD_REQUEST);
                }
                let child_value: serde_json::Value =
                    serde_json::from_str(&child).map_err(|_| StatusCode::BAD_REQUEST)?;
                if child_value
                    .get("mediaType")
                    .and_then(serde_json::Value::as_str)
                    != Some(child_media_type)
                {
                    return Err(StatusCode::BAD_REQUEST);
                }
                let child_size = child.len() as u64;
                if child_size > tree_bytes_budget {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                tree_bytes_budget -= child_size;
                manifest_metadata
                    .insert(digest.to_owned(), (child_media_type.to_owned(), child_size));
                pending.push((child.into_bytes(), depth + 1));
            }
            continue;
        }

        let config = manifest.get("config").ok_or(StatusCode::BAD_REQUEST)?;
        let layers = manifest
            .get("layers")
            .and_then(serde_json::Value::as_array)
            .ok_or(StatusCode::BAD_REQUEST)?;
        let descriptor_count = layers.len().saturating_add(1);
        if descriptor_count > descriptor_budget {
            return Err(StatusCode::BAD_REQUEST);
        }
        descriptor_budget -= descriptor_count;
        for descriptor in std::iter::once(config).chain(layers) {
            let (_blob_media_type, digest, expected_size) = validate_oci_descriptor(descriptor)?;
            if let Some(verified_size) = verified_blobs.get(digest) {
                if *verified_size != expected_size {
                    return Err(StatusCode::BAD_REQUEST);
                }
                continue;
            }
            if expected_size > verification_bytes_budget {
                return Err(StatusCode::PAYLOAD_TOO_LARGE);
            }
            verification_bytes_budget -= expected_size;
            let verified = store
                .verify_oci_blob_descriptor(repository, digest, expected_size)
                .await
                .map_err(|error| {
                    tracing::error!("verify OCI blob {repository}/{digest}: {error:#}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            if !verified {
                return Err(StatusCode::BAD_REQUEST);
            }
            verified_blobs.insert(digest.to_owned(), expected_size);
        }
    }
    Ok(())
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
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    auth.authorize_global_read(&headers)?;
    let repos = store
        .list_oci_repos()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({
        "repositories": repos
    })))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use mgc_oidc::{OidcClaims, claims::Audience};

    #[test]
    fn route_parser_preserves_repository_components_that_match_sections() {
        let cases = [
            (
                "team/manifests/manifests/latest",
                "team/manifests",
                "manifests",
                "/latest",
            ),
            (
                "team/blobs/blobs/sha256:abc",
                "team/blobs",
                "blobs",
                "/sha256:abc",
            ),
            (
                "team/tags/list/manifests/latest",
                "team/tags/list",
                "manifests",
                "/latest",
            ),
            (
                "team/tags/list/tags/list",
                "team/tags/list",
                "tags/list",
                "",
            ),
            (
                "team/app/manifests/blobs",
                "team/app",
                "manifests",
                "/blobs",
            ),
            (
                "team/app/manifests/manifests",
                "team/app",
                "manifests",
                "/manifests",
            ),
        ];

        for (path, expected_name, expected_section, expected_tail) in cases {
            assert_eq!(
                split_oci(path),
                Some((expected_name, expected_section, expected_tail))
            );
        }
        assert_eq!(split_oci("team/app/blobs"), None);
        assert_eq!(split_oci("team/app/manifests"), None);
    }

    #[tokio::test]
    async fn digest_reference_mismatch_precedes_trusted_signing_and_persistence() {
        let tmp = tempfile::tempdir().unwrap();
        let store = RegistryStore::new(tmp.path()).await.unwrap();
        let repository = "ai/models/weights";
        let config = b"config";
        let config_digest = format!("sha256:{}", hex::encode(Sha256::digest(config)));
        store
            .put_oci_blob(repository, &config_digest, config)
            .await
            .unwrap();
        let body = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.oci.empty.v1+json",
                "digest": config_digest,
                "size": config.len()
            },
            "layers": []
        }))
        .unwrap();
        let wrong_reference = format!("sha256:{}", "0".repeat(64));
        let identity = OidcClaims {
            iss: "https://token.actions.githubusercontent.com".into(),
            sub: "repo:acme/widgets:ref:refs/heads/main".into(),
            aud: Audience::Single("https://registry.example".into()),
            exp: 1_900_000_000,
            iat: 1_800_000_000,
            repository: Some("acme/widgets".into()),
            job_workflow_ref: None,
            workflow_ref: None,
            event_name: Some("push".into()),
        };
        let key = mgc_crypto::KeyPair::generate().unwrap();

        let result = manifest_put(
            &store,
            repository,
            &wrong_reference,
            Bytes::from(body),
            Some((identity, 1, "not-a-jwt".into())),
            Some(&key),
        )
        .await;

        assert!(matches!(result, Err(StatusCode::BAD_REQUEST)));
        assert!(
            store
                .get_oci_manifest(repository, &wrong_reference)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .trusted_attestations("oci:ai/models/weights")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn upload_finalization_publishes_complete_chunk_sequence() {
        let tmp = tempfile::tempdir().unwrap();
        let store = RegistryStore::new(tmp.path()).await.unwrap();
        let repository = "ai/models/weights";
        let uuid = "test-upload";
        let bytes = b"first patch and final put bytes";
        let split = 10;
        store.create_oci_upload(repository, uuid).await.unwrap();
        store
            .append_oci_upload(repository, uuid, 0, &bytes[..split])
            .await
            .unwrap();
        let offset = store
            .oci_upload_offset(repository, uuid)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(offset, split as i64);
        store
            .append_oci_upload(repository, uuid, offset, &bytes[split..])
            .await
            .unwrap();
        let digest = format!("sha256:{}", hex::encode(Sha256::digest(bytes)));

        store
            .finalize_oci_upload(repository, uuid, &digest)
            .await
            .unwrap();

        assert_eq!(
            store.get_oci_blob(repository, &digest).await.unwrap(),
            Some(bytes.to_vec())
        );
        assert!(
            store
                .oci_upload_path(repository, uuid)
                .await
                .unwrap()
                .is_none()
        );
    }
}
