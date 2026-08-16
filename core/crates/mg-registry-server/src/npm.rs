use crate::{model::*, AppState};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post, put},
    Router,
};
use serde::Deserialize;
use std::collections::HashMap;
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
        .route("/-/whoami", get(whoami))
        .route("/-/v1/search", get(search))
        .route("/-/v1/publish", post(batch_publish))
}

fn scoped_full(scope: &str, name: &str) -> String {
    format!("@{}/{}", scope.trim_start_matches('@'), name)
}

async fn get_package_scoped(
    State(state): State<AppState>,
    Path((scope, name)): Path<(String, String)>,
) -> Result<Json<Package>, StatusCode> {
    let result = get_package(State(state), Path(scoped_full(&scope, &name))).await;
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
    Path((scope, name)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Package>, StatusCode> {
    let result = publish_package(
        State(state),
        headers,
        Path(scoped_full(&scope, &name)),
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
    Path((scope, name, filename)): Path<(String, String, String)>,
) -> Result<Response, StatusCode> {
    let result = download_tarball(State(state), Path((scoped_full(&scope, &name), filename.clone()))).await;
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
    Path((scope, name, filename)): Path<(String, String, String)>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = upload_tarball(
        State(state),
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
    Path((scope, name, filename)): Path<(String, String, String)>,
) -> Result<StatusCode, StatusCode> {
    delete_package_version_route(State(state), Path((scoped_full(&scope, &name), filename))).await
}

// === Package fetching ===

async fn get_package(
    State((store, _auth)): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Package>, StatusCode> {
    match store.get_package(&name).await {
        Ok(Some(pkg)) => Ok(Json(pkg)),
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
    body: Bytes,
) -> Result<Json<Package>, StatusCode> {
    // npm CLI thật gửi metadata + _attachments (tarball base64) trong 1 PUT
    use base64::Engine;
    use sha2::{Digest, Sha512};

    let mut doc: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Lưu tarball từ _attachments → blob content-addressed, gắn integrity vào dist.
    // Fail-closed: attachment sai (thiếu data / base64 hỏng) → từ chối, không publish "mù"
    let mut attachments: HashMap<String, Vec<u8>> = HashMap::new();
    if let Some(atts) = doc.get("_attachments").and_then(|a| a.as_object()) {
        for (filename, att) in atts {
            let data = att
                .get("data")
                .and_then(|d| d.as_str())
                .ok_or(StatusCode::BAD_REQUEST)?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            attachments.insert(filename.clone(), bytes);
        }
    }
    let (blob_filename, blob) = match attachments.into_iter().next() {
        Some(kv) => kv,
        None => (String::new(), Vec::new()),
    };
    if !blob.is_empty() {
        let mut hasher = Sha512::new();
        hasher.update(&blob);
        let b64 = base64::engine::general_purpose::STANDARD.encode(hasher.finalize());
        let digest = format!("sha512-{b64}");
        store
            .put_blob(&digest, &blob)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(versions) = doc.get_mut("versions").and_then(|v| v.as_object_mut()) {
            // F2 fix: rewrite tarball theo Host header của chính registry — không tin URL
            // client gửi (client có thể publish qua registry khác → install sẽ chạy ra ngoài)
            let host = headers
                .get("host")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("localhost");
            for v in versions.values_mut() {
                if let Some(dist) = v.get_mut("dist") {
                    dist["integrity"] = serde_json::Value::String(digest.clone());
                    dist["tarball"] = serde_json::Value::String(format!(
                        "http://{host}/{}/-/{blob_filename}",
                        name
                    ));
                }
            }
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

    // Verify auth: fail-closed khi đã cấu hình admin token (registry private)
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    if auth.admin_token.is_some() {
        let user = token.and_then(|t| auth.verify_token(t)).ok_or_else(|| {
            warn!("publish {}: token rejected", name);
            StatusCode::UNAUTHORIZED
        })?;
        if !auth.can_publish(&user, &name) {
            warn!(
                "publish {}: user {} denied, scopes {:?}",
                name, user.name, user.scopes
            );
            return Err(StatusCode::FORBIDDEN);
        }
    } else if token.is_some() && auth.verify_token(token.unwrap()).is_none() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    store
        .put_package(&pkg)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let version = pkg
        .versions
        .keys()
        .next()
        .cloned()
        .or_else(|| {
            let v = pkg.dist_tags.get("latest").cloned();
            v
        });
    let _ = store.audit("publish", &pkg.name, version.as_deref(), None).await;
    Ok(Json(pkg))
}

// === Tarball download ===

async fn download_tarball(
    State((store, _)): State<AppState>,
    Path((name, filename)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    // filename: :unscoped-name-:version.tgz → version → dist.integrity → blob
    let unscoped = name.rsplit('/').next().unwrap_or(&name);
    let version = filename
        .strip_suffix(".tgz")
        .and_then(|s| s.strip_prefix(&format!("{}-", unscoped)));
    if let Some(version) = version {
        if let Some(pkg) = store.get_package(&name).await.ok().flatten() {
            if let Some(v) = pkg.versions.get(version) {
                let digest = &v.dist.integrity;
                if !digest.is_empty() {
                    if let Some(data) = store.get_blob(digest).await.ok().flatten() {
                        let mut resp = axum::response::Response::new(axum::body::Body::from(data));
                        resp.headers_mut().insert(
                            axum::http::header::CONTENT_TYPE,
                            HeaderValue::from_static("application/octet-stream"),
                        );
                        resp.headers_mut().insert(
                            "content-disposition",
                            HeaderValue::from_str(&format!(
                                "attachment; filename=\"{}\"",
                                filename
                            ))
                            .unwrap(),
                        );
                        return Ok(resp.into_response());
                    }
                }
            }
        }
    }

    Err(StatusCode::NOT_FOUND)
}

// === Tarball upload ===

async fn upload_tarball(
    State((store, _)): State<AppState>,
    Path((_name, _filename)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, StatusCode> {
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
    State((store, _)): State<AppState>,
    Path((name, filename)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
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
    State((store, _)): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<DistTagQuery>,
) -> Result<Json<HashMap<String, String>>, StatusCode> {
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
    State((store, _)): State<AppState>,
    Path((name, tag)): Path<(String, String)>,
    Json(body): Json<SetDistTagBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut pkg = store
        .get_package(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !pkg.versions.contains_key(&body.version) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let version = body.version;
    pkg.dist_tags.insert(tag.clone(), version.clone());
    store
        .put_package(&pkg)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({"tag": tag, "version": version})))
}

async fn delete_dist_tag(
    State((store, _)): State<AppState>,
    Path((name, tag)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
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
}

async fn adduser(
    State((_, auth)): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<AddUserBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let name = name.strip_prefix("org.couchdb.user:").unwrap_or(&name);
    // ponytail: scopes tin client, đủ cho registry nội bộ; admin cấp scope khi cần chặt
    let user = crate::auth::User {
        name: name.to_string(),
        is_admin: false,
        role: crate::auth::UserRole::Publisher,
        scopes: body.scopes,
        password: Some(body.password),
        email: body.email,
    };
    let token = uuid::Uuid::new_v4().to_string();
    auth.add_user(token.clone(), user);

    Ok(Json(serde_json::json!({
        "ok": true,
        "username": name,
        "token": token
    })))
}

async fn get_dist_tags_scoped(
    State(state): State<AppState>,
    Path((scope, name)): Path<(String, String)>,
    Query(query): Query<DistTagQuery>,
) -> Result<Json<HashMap<String, String>>, StatusCode> {
    get_dist_tags(State(state), Path(scoped_full(&scope, &name)), Query(query)).await
}

async fn set_dist_tag_scoped(
    State(state): State<AppState>,
    Path((scope, name, tag)): Path<(String, String, String)>,
    Json(body): Json<SetDistTagBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    set_dist_tag(
        State(state),
        Path((scoped_full(&scope, &name), tag)),
        Json(body),
    )
    .await
}

async fn delete_dist_tag_scoped(
    State(state): State<AppState>,
    Path((scope, name, tag)): Path<(String, String, String)>,
) -> Result<StatusCode, StatusCode> {
    delete_dist_tag(State(state), Path((scoped_full(&scope, &name), tag))).await
}

async fn delete_user(
    State((_, auth)): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    if !matches!(auth.admin_token.as_deref(), Some(t) if Some(t) == token) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let name = name
        .strip_prefix("org.couchdb.user:")
        .unwrap_or(&name)
        .to_string();
    if !auth.remove_user(&name).await.unwrap_or(false) {
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

    if let Some(token) = token {
        if let Some(user) = auth.verify_token(&token) {
            return Ok(Json(serde_json::json!({
                "username": user.name,
                "is_admin": user.is_admin
            })));
        }
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
    State((store, _)): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResult>, StatusCode> {
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
