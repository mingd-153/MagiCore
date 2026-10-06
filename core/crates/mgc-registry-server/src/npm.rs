use crate::{AppState, model::*};
use axum::{
    Router,
    body::Bytes,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post, put},
};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use tracing::warn;

/// npm API routes — alias không prefix (chuẩn npm client: PUT /:name) + /npm/ (nội bộ)
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/npm/:name", get(get_package).put(publish_package))
        .route("/:name", get(get_package).put(publish_package))
        .route(
            "/:scope/:name",
            get(get_package_scoped).put(publish_package_scoped),
        )
        .route(
            "/npm/:name/-/:filename",
            get(download_tarball).delete(delete_package_version_route),
        )
        .route("/npm/:name/-/:filename", put(upload_tarball))
        .route(
            "/:name/-/:filename",
            get(download_tarball)
                .put(upload_tarball)
                .delete(delete_package_version_route),
        )
        .route(
            "/:scope/:name/-/:filename",
            get(download_tarball_scoped).delete(delete_package_version_scoped),
        )
        .route("/:scope/:name/-/:filename", put(upload_tarball_scoped))
        .route(
            "/-/package/:name/dist-tags/:tag",
            put(set_dist_tag).delete(delete_dist_tag),
        )
        .route("/-/package/:name/dist-tags", get(get_dist_tags))
        .route(
            "/-/package/:scope/:name/dist-tags/:tag",
            put(set_dist_tag_scoped).delete(delete_dist_tag_scoped),
        )
        .route(
            "/-/package/:scope/:name/dist-tags",
            get(get_dist_tags_scoped),
        )
        .route("/-/user/:name", put(adduser).delete(delete_user))
        .route("/-/user/token/:token", delete(revoke_token))
        .route("/-/whoami", get(whoami))
        .route("/-/v1/search", get(search))
        .route("/-/v1/publish", post(batch_publish))
}

fn scoped_full(scope: &str, name: &str) -> String {
    format!("@{}/{}", scope.trim_start_matches('@'), name)
}

async fn get_package_scoped(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((scope, name)): Path<(String, String)>,
) -> Result<Json<Package>, StatusCode> {
    let result = get_package(State(state), headers, Path(scoped_full(&scope, &name))).await;
    eprintln!(
        "get_package_scoped: scope={} name={} result={:?}",
        scope,
        name,
        result.as_ref().map(|p| p.name.clone())
    );
    result
}

async fn publish_package_scoped(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Path((scope, name)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Package>, StatusCode> {
    let result = publish_package(
        State(state),
        headers,
        Path(scoped_full(&scope, &name)),
        peer,
        body,
    )
    .await
    .inspect_err(|e| eprintln!("publish_package_scoped error: {e:?}"));
    eprintln!(
        "publish_package_scoped: scope={} name={} result={:?}",
        scope,
        name,
        result.as_ref().map(|p| p.name.clone())
    );
    result
}

async fn download_tarball_scoped(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((scope, name, filename)): Path<(String, String, String)>,
) -> Result<Response, StatusCode> {
    let result = download_tarball(
        State(state),
        headers,
        Path((scoped_full(&scope, &name), filename.clone())),
    )
    .await;
    eprintln!(
        "download_tarball_scoped: scope={} name={} filename={} status={:?}",
        scope,
        name,
        filename,
        result.as_ref().map(|_| "ok")
    );
    result
}

async fn upload_tarball_scoped(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Path((scope, name, filename)): Path<(String, String, String)>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = upload_tarball(
        State(state),
        headers,
        peer,
        Path((scoped_full(&scope, &name), filename.clone())),
        body,
    )
    .await;
    eprintln!(
        "upload_tarball_scoped: scope={} name={} filename={} result={:?}",
        scope,
        name,
        filename,
        result.as_ref()
    );
    result
}

async fn delete_package_version_scoped(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Path((scope, name, filename)): Path<(String, String, String)>,
) -> Result<StatusCode, StatusCode> {
    delete_package_version_route(
        State(state),
        headers,
        peer,
        Path((scoped_full(&scope, &name), filename)),
    )
    .await
}

// === Package fetching ===

/// Rewrite `dist.tarball` theo Host header của request hiện tại — store lưu URL
/// của registry cũ (host:port lúc publish); đọc từ host/port khác phải trả URL
/// đúng chỗ này, nếu không client fetch tarball ra registry sai.
/// Scheme follows `X-Forwarded-Proto` (TLS-terminating proxies like the
/// bundled nginx set it); without it we keep plain `http` (local/default
/// deployments serve HTTP directly — advertising `https` there would hand
/// clients an unreachable URL). Only `http`/`https` values are honored.
/// Dùng scheme từ proxy TLS; mặc định HTTP cho triển khai local và chỉ nhận HTTP/HTTPS.
fn rewrite_tarball_host(pkg: &mut Package, host: &str, scheme: &str) {
    let scheme = match scheme {
        "https" | "http" => scheme,
        _ => "http",
    };
    for v in pkg.versions.values_mut() {
        if let Some(filename) = v.dist.tarball.rsplit('/').next() {
            v.dist.tarball = format!("{scheme}://{host}/{}/-/{filename}", pkg.name);
        }
    }
}

async fn get_package(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Package>, StatusCode> {
    auth.authorize_package_read(&headers, &name)?;
    match store.get_package(&name).await {
        Ok(Some(mut pkg)) => {
            let host = headers
                .get("host")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("localhost");
            let scheme = headers
                .get("x-forwarded-proto")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("http");
            rewrite_tarball_host(&mut pkg, host, scheme);
            Ok(Json(pkg))
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            warn!("Failed to get package {}: {}", name, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// === Package publishing ===

async fn publish_package(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    peer: Option<ConnectInfo<SocketAddr>>,
    body: Bytes,
) -> Result<Json<Package>, StatusCode> {
    crate::trusted::scoped_package("npm", &name)?;
    let trusted_identity = auth
        .authorize_package_write_from(&headers, &name, peer.map(|ConnectInfo(address)| address))
        .await?;
    // npm CLI thật gửi metadata + _attachments (tarball base64) trong 1 PUT
    use base64::Engine;
    use sha2::{Digest, Sha512};

    let mut doc: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Lưu tarball từ _attachments → blob content-addressed, gắn integrity vào dist.
    // Fail-closed: attachment sai (thiếu data / base64 hỏng) → từ chối, không publish "mù"
    let mut attachments = Vec::new();
    if let Some(atts) = doc.get("_attachments").and_then(|a| a.as_object()) {
        for (filename, att) in atts {
            let data = att
                .get("data")
                .and_then(|d| d.as_str())
                .ok_or(StatusCode::BAD_REQUEST)?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            attachments.push((filename.clone(), bytes));
        }
    }
    if attachments.len() > 1 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some((_attachment_filename, blob)) = attachments.pop() {
        let versions = doc
            .get("versions")
            .and_then(|versions| versions.as_object())
            .ok_or(StatusCode::BAD_REQUEST)?;
        if versions.len() != 1 || blob.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        let mut hasher = Sha512::new();
        hasher.update(&blob);
        let b64 = base64::engine::general_purpose::STANDARD.encode(hasher.finalize());
        let digest = format!("sha512-{b64}");
        store
            .put_blob(&digest, &blob)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        // Bind the single attachment to the single version and registry origin.
        // Gắn attachment duy nhất cho version duy nhất và origin của registry.
        let host = headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("localhost");
        let versions = doc
            .get_mut("versions")
            .and_then(|versions| versions.as_object_mut())
            .ok_or(StatusCode::BAD_REQUEST)?;
        let (version_key, version) = versions.iter_mut().next().ok_or(StatusCode::BAD_REQUEST)?;
        let version_key = version_key.clone();
        if let Some(dist) = version.get_mut("dist") {
            dist["integrity"] = serde_json::Value::String(digest);
            let unscoped_name = name.rsplit('/').next().unwrap_or(&name);
            let filename = format!("{unscoped_name}-{version_key}.tgz");
            dist["tarball"] =
                serde_json::Value::String(format!("http://{host}/{}/-/{filename}", name));
        } else {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let pkg: Package = serde_json::from_value(doc).map_err(|e| {
        warn!("publish {}: body parse fail: {e}", name);
        StatusCode::BAD_REQUEST
    })?;

    // Verify name matches
    if pkg.name != name {
        return Err(StatusCode::BAD_REQUEST);
    }

    if let Some((identity, binding_generation, sigstore_oidc_token)) = trusted_identity {
        if pkg.versions.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        for version in pkg.versions.values() {
            if !crate::trusted::is_valid_sha512_integrity(&version.dist.integrity) {
                return Err(StatusCode::BAD_REQUEST);
            }
            if store
                .get_blob(&version.dist.integrity)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .is_none()
            {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        let key = auth.attestation_key().ok_or(StatusCode::NOT_FOUND)?;
        let mut public_attestations = HashMap::new();
        for (version, package_version) in &pkg.versions {
            let blob = store
                .get_blob(&package_version.dist.integrity)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::BAD_REQUEST)?;
            let artifact_sha256 = hex::encode(sha2::Sha256::digest(&blob));
            let bundle = crate::trusted::sign_sigstore_attestation(
                &sigstore_oidc_token,
                &identity,
                &pkg.name,
                version,
                &format!("{}@{}", pkg.name, version),
                &package_version.dist.integrity,
                &artifact_sha256,
            )
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            public_attestations.insert(version.clone(), (artifact_sha256, bundle));
        }
        store
            .put_trusted_package(
                &pkg,
                &identity,
                key,
                binding_generation,
                &public_attestations,
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    } else {
        store
            .put_package(&pkg)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let version = pkg
        .versions
        .keys()
        .next()
        .cloned()
        .or_else(|| pkg.dist_tags.get("latest").cloned());
    let _ = store
        .audit("publish", &pkg.name, version.as_deref(), None)
        .await;
    Ok(Json(pkg))
}

fn content_disposition(filename: &str) -> Result<HeaderValue, StatusCode> {
    HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// === Tarball download ===

async fn download_tarball(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    Path((name, filename)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    auth.authorize_package_read(&headers, &name)?;
    // filename: :unscoped-name-:version.tgz → version → dist.integrity → blob
    let unscoped = name.rsplit('/').next().unwrap_or(&name);
    let version = filename
        .strip_suffix(".tgz")
        .and_then(|s| s.strip_prefix(&format!("{}-", unscoped)));
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Some(version) = version
        && let Some(pkg) = store.get_package(&name).await.ok().flatten()
        && let Some(v) = pkg.versions.get(version)
    {
        let digest = &v.dist.integrity;
        if !digest.is_empty() {
            if let Some(data) = store.get_blob(digest).await.ok().flatten() {
                let mut resp = axum::response::Response::new(axum::body::Body::from(data));
                resp.headers_mut().insert(
                    axum::http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/octet-stream"),
                );
                resp.headers_mut()
                    .insert("content-disposition", content_disposition(&filename)?);
                return Ok(resp.into_response());
            }
            // ITEM 4: blob miss → proxy tarball từ upstream, cache vào store
            // Upstream bytes are verified against the declared digest BEFORE
            // caching and serving — a mismatched upstream response is dropped
            // (fail-closed) instead of being cached as poison.
            // Bytes từ upstream được đối chiếu digest khai TRƯỚC khi cache và
            // serve — lệch digest thì bỏ (fail-closed), không cache dữ liệu bẩn.
            if let Ok(Some(data)) = store.fetch_upstream_tarball(&v.dist.tarball).await {
                match store.put_blob(digest, &data).await {
                    Ok(()) => {}
                    Err(e) => {
                        tracing::warn!("upstream tarball digest mismatch, not cached: {e:#}");
                        return Err(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                }
                let mut resp = axum::response::Response::new(axum::body::Body::from(data));
                resp.headers_mut().insert(
                    axum::http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/octet-stream"),
                );
                resp.headers_mut()
                    .insert("content-disposition", content_disposition(&filename)?);
                return Ok(resp.into_response());
            }
        }
    }

    Err(StatusCode::NOT_FOUND)
}

// === Tarball upload ===

async fn upload_tarball(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Path((_name, _filename)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, StatusCode> {
    auth.authorize_npm_write_from(&headers, &_name, peer.map(|ConnectInfo(address)| address))
        .await?;
    use sha2::{Digest, Sha512};

    let mut hasher = Sha512::new();
    hasher.update(&body);
    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        hasher.finalize(),
    );
    let digest = format!("sha512-{b64}");
    store
        .put_blob(&digest, &body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = store
        .audit("upload-tarball", &_name, Some(&_filename), None)
        .await;
    Ok(Json(
        serde_json::json!({ "ok": true, "digest": digest, "size": body.len() }),
    ))
}

// === Tarball/version delete (npm unpublish) ===

async fn delete_package_version_route(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Path((name, filename)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    auth.authorize_npm_delete_from(&headers, &name, peer.map(|ConnectInfo(address)| address))
        .await?;
    // filename: :name-:version.tgz → trích version
    let version = filename
        .strip_suffix(".tgz")
        .and_then(|s| s.strip_prefix(&format!("{}-", name)));
    let Some(version) = version else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let res = store
        .delete_package_version(&name, version)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .then_some(StatusCode::NO_CONTENT)
        .ok_or(StatusCode::NOT_FOUND)?;
    let _ = store.audit("delete", &name, Some(version), None).await;
    Ok(res)
}

// === Dist-tags ===

#[derive(Deserialize)]
struct DistTagQuery {
    tag: Option<String>,
}

async fn get_dist_tags(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(query): Query<DistTagQuery>,
) -> Result<Json<HashMap<String, String>>, StatusCode> {
    auth.authorize_package_read(&headers, &name)?;
    if let Some(pkg) = store
        .get_package(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        if let Some(tag) = query.tag {
            if let Some(version) = pkg.dist_tags.get(&tag) {
                let mut result = HashMap::new();
                result.insert(tag, version.clone());
                return Ok(Json(result));
            }
            return Err(StatusCode::NOT_FOUND);
        }
        Ok(Json(pkg.dist_tags))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Deserialize)]
struct SetDistTagBody {
    version: String,
}

async fn set_dist_tag(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Path((name, tag)): Path<(String, String)>,
    Json(body): Json<SetDistTagBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    auth.authorize_npm_write_from(&headers, &name, peer.map(|ConnectInfo(address)| address))
        .await?;
    let mut pkg = store
        .get_package(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !pkg.versions.contains_key(&body.version) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let version = body.version;
    if let Some(binding_generation) = auth.trusted_binding_generation(&headers, &name) {
        let updated = store
            .set_trusted_dist_tag(&name, &tag, &version, binding_generation)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if !updated {
            return Err(StatusCode::FORBIDDEN);
        }
        return Ok(Json(serde_json::json!({"tag": tag, "version": version})));
    }
    pkg.dist_tags.insert(tag.clone(), version.clone());
    store
        .put_package(&pkg)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({"tag": tag, "version": version})))
}

async fn delete_dist_tag(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Path((name, tag)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    auth.authorize_npm_delete_from(&headers, &name, peer.map(|ConnectInfo(address)| address))
        .await?;
    let mut pkg = store
        .get_package(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if pkg.dist_tags.remove(&tag).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    store
        .put_package(&pkg)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

// === User management ===

#[derive(Deserialize)]
struct AddUserBody {
    password: String,
    email: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    role: Option<String>,
}

async fn adduser(
    State((_, auth)): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(body): Json<AddUserBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Only administrators grant publishing rights; public accounts are viewers.
    // Chỉ admin cấp quyền publish; tài khoản công khai chỉ có quyền đọc.
    let privileged = auth
        .authenticate_headers(&headers)
        .is_some_and(|user| user.is_admin);
    if !privileged
        && (!body.scopes.is_empty() || body.role.as_deref().is_some_and(|r| r != "viewer"))
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let name = name.strip_prefix("org.couchdb.user:").unwrap_or(&name);
    // Reject unknown roles instead of silently downgrading a provisioning request.
    // Báo lỗi role không hợp lệ để admin không tưởng đã cấp đúng quyền.
    let role = match body.role.as_deref().unwrap_or("viewer") {
        "viewer" => crate::auth::UserRole::Viewer,
        "publisher" => crate::auth::UserRole::Publisher,
        "admin" => crate::auth::UserRole::Admin,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    if name.trim().is_empty() || body.password.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let user = crate::auth::User {
        name: name.to_string(),
        is_admin: privileged && role == crate::auth::UserRole::Admin,
        role,
        scopes: body.scopes,
        password: Some(body.password),
        email: body.email,
    };
    let token = uuid::Uuid::new_v4().to_string();
    auth.register_user(token.clone(), user).await?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "username": name,
        "token": token
    })))
}

async fn get_dist_tags_scoped(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((scope, name)): Path<(String, String)>,
    Query(query): Query<DistTagQuery>,
) -> Result<Json<HashMap<String, String>>, StatusCode> {
    get_dist_tags(
        State(state),
        headers,
        Path(scoped_full(&scope, &name)),
        Query(query),
    )
    .await
}

async fn set_dist_tag_scoped(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Path((scope, name, tag)): Path<(String, String, String)>,
    Json(body): Json<SetDistTagBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    set_dist_tag(
        State(state),
        headers,
        peer,
        Path((scoped_full(&scope, &name), tag)),
        Json(body),
    )
    .await
}

async fn delete_dist_tag_scoped(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Path((scope, name, tag)): Path<(String, String, String)>,
) -> Result<StatusCode, StatusCode> {
    delete_dist_tag(
        State(state),
        headers,
        peer,
        Path((scoped_full(&scope, &name), tag)),
    )
    .await
}

async fn delete_user(
    State((_, auth)): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    auth.authorize_admin(&headers)?;
    let name = name
        .strip_prefix("org.couchdb.user:")
        .unwrap_or(&name)
        .to_string();
    if !auth.remove_user(&name).await.unwrap_or(false) {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// ITEM 6: revoke token — chỉ admin (admin_token)
async fn revoke_token(
    State((_, auth)): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    auth.authorize_admin(&headers)?;
    if !auth.remove_token(&token).await.unwrap_or(false) {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn whoami(
    State((_, auth)): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(String::from);

    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Some(token) = token
        && let Some(user) = auth.verify_token(&token)
    {
        return Ok(Json(serde_json::json!({
            "username": user.name,
            "is_admin": user.is_admin
        })));
    }

    Err(StatusCode::UNAUTHORIZED)
}

// === Search ===

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    size: Option<u32>,
    from: Option<u32>,
}

async fn search(
    State((store, auth)): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResult>, StatusCode> {
    auth.authorize_global_read(&headers)?;
    let limit = query.size.unwrap_or(20).min(100);
    let offset = query.from.unwrap_or(0);
    let results = store
        .search_packages(&query.q, limit, offset)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let total = results.len() as u64;

    Ok(Json(SearchResult {
        objects: results,
        total,
        time: "0ms".to_string(),
    }))
}

// === Batch publish ===

async fn batch_publish(
    State((_, _)): State<AppState>,
    _body: Bytes,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

#[cfg(test)]
#[path = "test/npm.rs"]
mod tarball_scheme_tests;
