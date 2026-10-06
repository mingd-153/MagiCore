//! Authentication and authorization for registry server
//! (Auth: Bearer token, Basic auth, scope-based access control — users persist qua storage)

use anyhow::Result;
use axum::{
    extract::{ConnectInfo, Extension, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::Engine;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::storage::RegistryStore;
use crate::trusted::{
    MAX_ACTIVE_TRUSTED_TOKENS, TRUSTED_TOKEN_TTL_SECS, TrustedPublishingConfig,
    claims_match_repository_binding,
};
use mgc_crypto::KeyPair;
use mgc_oidc::OidcClaims;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct TrustedToken {
    user: User,
    package: String,
    identity: OidcClaims,
    sigstore_oidc_token: String,
    binding_generation: u64,
    expires_at: Instant,
}

/// Authentication service — users persist trong SQLite, cache Mutex cho đọc nhanh
pub struct AuthService {
    pub admin_token: Option<String>,
    users: Mutex<HashMap<String, User>>, // token -> user (cache; source of truth = DB)
    pub scopes: HashMap<String, Vec<String>>, // scope -> allowed packages
    store: Arc<RegistryStore>,
    trusted_config: Option<TrustedPublishingConfig>,
    trusted_tokens: Mutex<HashMap<String, TrustedToken>>,
    attestation_key: Option<KeyPair>,
    allow_insecure_trusted_http: bool,
    trusted_proxy_ips: Vec<IpAddr>,
}

impl Clone for AuthService {
    fn clone(&self) -> Self {
        Self {
            admin_token: self.admin_token.clone(),
            users: Mutex::new(self.users_guard().clone()),
            scopes: self.scopes.clone(),
            store: self.store.clone(),
            trusted_config: self.trusted_config.clone(),
            trusted_tokens: Mutex::new(self.trusted_tokens_guard().clone()),
            attestation_key: self.attestation_key.clone(),
            allow_insecure_trusted_http: self.allow_insecure_trusted_http,
            trusted_proxy_ips: self.trusted_proxy_ips.clone(),
        }
    }
}

impl AuthService {
    pub fn new(admin_token: Option<String>, store: Arc<RegistryStore>) -> Self {
        Self {
            admin_token,
            users: Mutex::new(HashMap::new()),
            scopes: HashMap::new(),
            store,
            trusted_config: None,
            trusted_tokens: Mutex::new(HashMap::new()),
            attestation_key: None,
            allow_insecure_trusted_http: false,
            trusted_proxy_ips: Vec::new(),
        }
    }

    pub fn with_trusted_publishing(
        mut self,
        config: Option<TrustedPublishingConfig>,
        key: Option<KeyPair>,
    ) -> Self {
        self.trusted_config = config;
        self.attestation_key = key;
        self
    }

    /// Permit HTTP trusted-token exchange only when the server binds loopback.
    /// Chỉ cho phép đổi token qua HTTP khi server bind vào loopback.
    pub fn allow_insecure_trusted_http(mut self, allow: bool) -> Self {
        self.allow_insecure_trusted_http = allow;
        self
    }

    /// Trust forwarded HTTPS only from explicitly configured proxy peers.
    /// Chỉ tin HTTPS chuyển tiếp từ địa chỉ peer proxy đã cấu hình cụ thể.
    pub fn trusted_proxy_ips(mut self, addresses: Vec<IpAddr>) -> Self {
        self.trusted_proxy_ips = addresses;
        self
    }

    pub fn trusted_transport_allowed(&self, headers: &HeaderMap, peer: Option<SocketAddr>) -> bool {
        let Some(peer) = peer else {
            return false;
        };
        (self.trusted_proxy_ips.contains(&peer.ip())
            && headers
                .get("x-forwarded-proto")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.eq_ignore_ascii_case("https")))
            || (self.allow_insecure_trusted_http && peer.ip().is_loopback())
    }

    fn has_active_trusted_token(&self, headers: &HeaderMap) -> bool {
        let Some(token) = trusted_token_secret(headers) else {
            return false;
        };
        let mut tokens = self.trusted_tokens_guard();
        tokens.retain(|_, value| value.expires_at > Instant::now());
        tokens.contains_key(&token)
    }

    fn trusted_tokens_guard(&self) -> MutexGuard<'_, HashMap<String, TrustedToken>> {
        self.trusted_tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn trusted_enabled(&self) -> bool {
        self.trusted_config.is_some() && self.attestation_key.is_some()
    }

    pub fn attestation_key(&self) -> Option<&KeyPair> {
        self.attestation_key.as_ref()
    }

    pub async fn exchange_trusted_token(
        &self,
        package: &str,
        jwt: &str,
        sigstore_jwt: &str,
    ) -> Result<(String, u64, OidcClaims), StatusCode> {
        let config = self.trusted_config.as_ref().ok_or(StatusCode::NOT_FOUND)?;
        if !self.trusted_enabled() {
            return Err(StatusCode::NOT_FOUND);
        }
        let issuers = config
            .issuers
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let claims = mgc_oidc::validate::validate_with_discovery(jwt, &issuers, &config.audience)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        let sigstore_claims =
            mgc_oidc::validate::validate_with_discovery(sigstore_jwt, &issuers, "sigstore")
                .await
                .map_err(|_| StatusCode::UNAUTHORIZED)?;
        if !claims.same_workload_identity(&sigstore_claims) {
            return Err(StatusCode::FORBIDDEN);
        }
        let (repository, binding_generation) = self
            .store
            .trusted_publisher_binding(package)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::FORBIDDEN)?;
        if !claims_match_repository_binding(&claims, &repository) {
            return Err(StatusCode::FORBIDDEN);
        }

        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let user = User {
            name: format!("oidc:{}", repository),
            is_admin: false,
            role: UserRole::Publisher,
            scopes: vec![package.to_string()],
            password: None,
            email: None,
        };
        let mut tokens = self.trusted_tokens_guard();
        tokens.retain(|_, token| token.expires_at > Instant::now());
        if tokens.len() >= MAX_ACTIVE_TRUSTED_TOKENS {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        tokens.insert(
            token.clone(),
            TrustedToken {
                user,
                package: package.to_string(),
                identity: claims.clone(),
                sigstore_oidc_token: sigstore_jwt.to_owned(),
                binding_generation,
                expires_at: Instant::now() + Duration::from_secs(TRUSTED_TOKEN_TTL_SECS),
            },
        );
        Ok((token, TRUSTED_TOKEN_TTL_SECS, claims))
    }

    pub fn trusted_identity(&self, headers: &HeaderMap, package: &str) -> Option<OidcClaims> {
        let token = trusted_token_secret(headers)?;
        let mut tokens = self.trusted_tokens_guard();
        tokens.retain(|_, value| value.expires_at > Instant::now());
        let trusted = tokens.get(&token)?;
        (trusted.package == package).then(|| trusted.identity.clone())
    }

    pub fn discard_trusted_token(&self, token: &str) {
        self.trusted_tokens_guard().remove(token);
    }

    pub async fn authorize_package_write(
        &self,
        headers: &HeaderMap,
        package: &str,
    ) -> Result<Option<(OidcClaims, u64, String)>, StatusCode> {
        self.authorize_package_write_from(headers, package, None)
            .await
    }

    pub async fn authorize_package_write_from(
        &self,
        headers: &HeaderMap,
        package: &str,
        peer: Option<SocketAddr>,
    ) -> Result<Option<(OidcClaims, u64, String)>, StatusCode> {
        let binding = self
            .store
            .trusted_publisher_binding(package)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(token) = trusted_token_secret(headers) {
            let trusted = {
                let mut tokens = self.trusted_tokens_guard();
                tokens.retain(|_, trusted| trusted.expires_at > Instant::now());
                tokens.get(&token).cloned()
            };
            if let Some(trusted) = trusted {
                if !self.trusted_transport_allowed(headers, peer) {
                    return Err(StatusCode::FORBIDDEN);
                }
                if trusted.package != package || !self.can_publish(&trusted.user, package) {
                    return Err(StatusCode::FORBIDDEN);
                }
                if binding.as_ref().is_none_or(|(repository, generation)| {
                    Some(repository.as_str()) != trusted.identity.repository.as_deref()
                        || *generation != trusted.binding_generation
                }) {
                    return Err(StatusCode::FORBIDDEN);
                }
                return Ok(Some((
                    trusted.identity.clone(),
                    trusted.binding_generation,
                    trusted.sigstore_oidc_token.clone(),
                )));
            }
        }
        if binding.is_some() {
            return Err(StatusCode::FORBIDDEN);
        }
        if self.admin_token.is_none() {
            return Ok(None);
        }
        let user = self
            .authenticate_headers(headers)
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if !self.can_publish(&user, package) {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(None)
    }

    pub fn trusted_binding_generation(&self, headers: &HeaderMap, package: &str) -> Option<u64> {
        let token = trusted_token_secret(headers)?;
        let mut tokens = self.trusted_tokens_guard();
        tokens.retain(|_, trusted| trusted.expires_at > Instant::now());
        let trusted = tokens.get(&token)?;
        (trusted.package == package).then_some(trusted.binding_generation)
    }

    fn users_guard(&self) -> MutexGuard<'_, HashMap<String, User>> {
        // Recover poisoned auth cache — khôi phục cache auth bị poison thay vì panic registry.
        self.users
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Nạp user từ DB vào cache — gọi lúc khởi động (10-task-plan: users sống qua restart)
    pub async fn load_from_db(&self) -> Result<()> {
        let rows = self.store.load_users().await?;
        let mut users = self.users_guard();
        users.clear();
        for (token, user) in rows {
            users.insert(token, user);
        }
        Ok(())
    }

    /// Add a user with token — ghi cả DB (persist) + cache
    pub fn add_user(&self, token: String, user: User) {
        self.users_guard().insert(token.clone(), user.clone());
        // fire-and-forget async — lỗi DB không chặn adduser (ghi log)
        let store = self.store.clone();
        tokio::spawn(async move {
            if let Err(e) = store.upsert_user(&token, &user).await {
                eprintln!("[auth] persist user {} failed: {e}", user.name);
            }
        });
    }

    /// Publish credentials only after durable insertion — chỉ cấp token sau khi lưu thành công.
    pub async fn register_user(&self, token: String, user: User) -> Result<(), StatusCode> {
        if !self
            .store
            .create_user(&token, &user)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        {
            return Err(StatusCode::CONFLICT);
        }
        self.users_guard().insert(token, user);
        Ok(())
    }

    /// Delete user — DB + cache
    pub async fn remove_user(&self, name: &str) -> Result<bool> {
        let removed = self.store.delete_user_by_name(name).await?;
        if removed {
            self.users_guard().retain(|_, u| u.name != name);
        }
        Ok(removed)
    }

    /// Revoke token (ITEM 6) — xóa cache + DB theo token
    pub async fn remove_token(&self, token: &str) -> Result<bool> {
        let removed = self.store.delete_user_by_token(token).await?;
        let removed_trusted = self.trusted_tokens_guard().remove(token).is_some();
        if removed {
            self.users_guard().remove(token);
        }
        Ok(removed || removed_trusted)
    }

    /// Xác thực username + password (Basic auth)
    pub fn verify_password(&self, username: &str, password: &str) -> Option<User> {
        self.users_guard()
            .values()
            // ponytail: scan tuyến tính, đủ cho registry private; index theo name khi scale
            .find(|u| u.name == username && u.password.as_deref() == Some(password))
            .cloned()
    }

    /// Verify token and return user (owned)
    pub fn verify_token(&self, token: &str) -> Option<User> {
        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if let Some(admin) = &self.admin_token
            && token == admin
        {
            return Some(User {
                name: "admin".to_string(),
                is_admin: true,
                role: UserRole::Admin,
                scopes: vec![],
                password: None,
                email: None,
            });
        }
        self.users_guard().get(token).cloned().or_else(|| {
            let mut trusted_tokens = self.trusted_tokens_guard();
            trusted_tokens.retain(|_, value| value.expires_at > Instant::now());
            trusted_tokens.get(token).map(|value| value.user.clone())
        })
    }

    /// Resolve Bearer/Basic consistently — xác thực chung cho mọi giao thức.
    pub fn authenticate_headers(&self, headers: &HeaderMap) -> Option<User> {
        let value = headers.get("authorization")?.to_str().ok()?;
        if let Some(token) = value.strip_prefix("Bearer ") {
            return self.verify_token(token);
        }
        let encoded = value.strip_prefix("Basic ")?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;
        let decoded = String::from_utf8(decoded).ok()?;
        let (name, password) = decoded.split_once(':')?;
        if name == "mgc-oidc" {
            let mut tokens = self.trusted_tokens_guard();
            tokens.retain(|_, trusted| trusted.expires_at > Instant::now());
            return tokens.get(password).map(|trusted| trusted.user.clone());
        }
        if self.admin_token.as_deref() == Some(password) {
            return self.verify_token(password);
        }
        self.verify_password(name, password)
    }

    /// Authorize before writes; retain explicit open mode.
    /// Kiểm quyền trước khi ghi; giữ chế độ registry mở hiện có.
    pub async fn authorize_write(
        &self,
        headers: &HeaderMap,
        package: &str,
    ) -> Result<(), StatusCode> {
        if self.trusted_identity(headers, package).is_some() {
            return Err(StatusCode::FORBIDDEN);
        }
        self.authorize_package_write(headers, package)
            .await
            .map(|_| ())
    }

    pub async fn authorize_npm_write(
        &self,
        headers: &HeaderMap,
        package: &str,
    ) -> Result<(), StatusCode> {
        self.authorize_npm_write_from(headers, package, None).await
    }

    pub async fn authorize_npm_write_from(
        &self,
        headers: &HeaderMap,
        package: &str,
        peer: Option<SocketAddr>,
    ) -> Result<(), StatusCode> {
        crate::trusted::scoped_package("npm", package)?;
        self.authorize_package_write_from(headers, package, peer)
            .await
            .map(|_| ())
    }

    pub async fn authorize_npm_delete(
        &self,
        headers: &HeaderMap,
        package: &str,
    ) -> Result<(), StatusCode> {
        self.authorize_npm_delete_from(headers, package, None).await
    }

    pub async fn authorize_npm_delete_from(
        &self,
        headers: &HeaderMap,
        package: &str,
        peer: Option<SocketAddr>,
    ) -> Result<(), StatusCode> {
        crate::trusted::scoped_package("npm", package)?;
        if self.trusted_identity(headers, package).is_some() {
            return Err(StatusCode::FORBIDDEN);
        }
        self.authorize_package_write_from(headers, package, peer)
            .await
            .map(|_| ())
    }

    /// Restrict authenticated reads to packages allowed by the caller's scope.
    /// Giới hạn đọc đã xác thực theo package được phép trong scope của caller.
    pub fn authorize_package_read(
        &self,
        headers: &HeaderMap,
        package: &str,
    ) -> Result<(), StatusCode> {
        if let Some(user) = self.authenticate_headers(headers) {
            return self
                .can_access(&user, package)
                .then_some(())
                .ok_or(StatusCode::FORBIDDEN);
        }
        if headers.contains_key("authorization") && self.admin_token.is_some() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(())
    }

    /// Authenticated non-admins cannot enumerate; anonymous reads follow registry visibility.
    /// User đã xác thực không phải admin không được liệt kê; user ẩn danh theo chính sách registry.
    pub fn authorize_global_read(&self, headers: &HeaderMap) -> Result<(), StatusCode> {
        if let Some(user) = self.authenticate_headers(headers) {
            return user.is_admin.then_some(()).ok_or(StatusCode::FORBIDDEN);
        }
        if headers.contains_key("authorization") && self.admin_token.is_some() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(())
    }

    /// Require a configured admin identity for account mutations — chỉ admin đã cấu hình được sửa tài khoản.
    pub fn authorize_admin(&self, headers: &HeaderMap) -> Result<(), StatusCode> {
        if self.admin_token.is_none() {
            return Err(StatusCode::FORBIDDEN);
        }
        let user = self
            .authenticate_headers(headers)
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if !user.is_admin {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(())
    }

    /// Check if user can access package (scope-based, glob: `@scope/*`, `*`)
    pub fn can_access(&self, user: &User, package: &str) -> bool {
        if user.is_admin {
            return true;
        }
        // User scope patterns trực tiếp (vd: "@magicore/*")
        for scope in &user.scopes {
            if scope_matches(scope, package) {
                return true;
            }
        }
        // Check scope mapping
        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        for scope in &user.scopes {
            if let Some(packages) = self.scopes.get(scope)
                && packages.iter().any(|p| scope_matches(p, package))
            {
                return true;
            }
        }
        false
    }

    /// Check if user can publish package (task #4: viewer = read-only)
    pub fn can_publish(&self, user: &User, package: &str) -> bool {
        user.role.can_publish() && self.can_access(user, package)
    }
}

fn trusted_token_secret(headers: &HeaderMap) -> Option<String> {
    let value = headers.get("authorization")?.to_str().ok()?;
    if let Some(token) = value.strip_prefix("Bearer ") {
        return Some(token.to_owned());
    }
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (username, password) = decoded.split_once(':')?;
    (username == "mgc-oidc").then(|| password.to_owned())
}

/// User representation — Debug never prints the password hash/secret.
/// Biểu diễn user — Debug không in hash/mật khẩu hay bí mật.
#[derive(Clone)]
pub struct User {
    pub name: String,
    pub is_admin: bool,
    pub role: UserRole,
    pub scopes: Vec<String>,
    pub password: Option<String>,
    pub email: Option<String>,
}

impl std::fmt::Debug for User {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("User")
            .field("name", &self.name)
            .field("is_admin", &self.is_admin)
            .field("role", &self.role)
            .field("scopes", &self.scopes)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("email", &self.email)
            .finish()
    }
}

/// RBAC role (task #4): viewer = read-only, publisher = read+write scoped, admin = all
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserRole {
    Viewer,
    Publisher,
    #[default]
    Admin,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserRole::Viewer => "viewer",
            UserRole::Publisher => "publisher",
            UserRole::Admin => "admin",
        }
    }

    pub fn can_publish(&self) -> bool {
        matches!(self, UserRole::Publisher | UserRole::Admin)
    }
}

impl FromStr for UserRole {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let role = match s {
            "publisher" => UserRole::Publisher,
            "admin" => UserRole::Admin,
            _ => UserRole::Viewer,
        };
        Ok(role)
    }
}

/// Extract auth from request headers
pub fn extract_auth(headers: &HeaderMap) -> Option<(String, String)> {
    // Check Authorization header
    if let Some(auth) = headers.get("authorization") {
        let auth_str = auth.to_str().ok()?;
        if let Some(token) = auth_str.strip_prefix("Bearer ") {
            return Some(("Bearer".to_string(), token.to_string()));
        }
        if let Some(encoded) = auth_str.strip_prefix("Basic ")
            && let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded)
        {
            let decoded_str = String::from_utf8(decoded).ok()?;
            let mut parts = decoded_str.splitn(2, ':');
            if let (Some(user), Some(pass)) = (parts.next(), parts.next()) {
                return Some((user.to_string(), pass.to_string()));
            }
        }
    }
    // Check for token in query (less secure, but supported)
    None
}

/// Auth middleware — dùng qua route_layer + Extension (không có State trong route_layer).
/// Trả 401 kèm WWW-Authenticate challenge (chuẩn OCI/Docker): client Basic/Bearer
/// đọc challenge rồi gửi credential — thiếu header khiến oras/pip từ chối retry.
pub async fn auth_middleware(
    axum::extract::Extension(auth): axum::extract::Extension<Arc<AuthService>>,
    request: Request,
    next: Next,
) -> Response {
    use axum::response::IntoResponse;
    // Adduser public (chuẩn npm): tạo credential mới, không cần token sẵn có
    if request.method() == axum::http::Method::PUT && request.uri().path().starts_with("/-/user/") {
        return next.run(request).await;
    }
    if request.method() == axum::http::Method::POST
        && request.uri().path() == "/-/v1/trusted-publish/token"
    {
        return next.run(request).await;
    }

    if request
        .headers()
        .get("authorization")
        .is_some_and(|value| value.to_str().is_err())
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if let Some(user) = auth.authenticate_headers(request.headers()) {
        let mut request = request;
        request.extensions_mut().insert(user);
        return next.run(request).await;
    }

    unauthorized_response()
}

/// Reject any request carrying an ephemeral trusted token over an unverified transport.
/// Từ chối mọi request mang token trusted tạm thời qua transport chưa được xác thực.
pub async fn trusted_transport_middleware(
    Extension(auth): Extension<Arc<AuthService>>,
    request: Request,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| *address);
    if auth.has_active_trusted_token(request.headers())
        && !auth.trusted_transport_allowed(request.headers(), peer)
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

/// 401 kèm WWW-Authenticate challenge (chuẩn OCI/Docker).
pub fn unauthorized_response() -> Response {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(
            "WWW-Authenticate",
            "Basic realm=\"magicore-registry\", Bearer realm=\"magicore-registry\"",
        )
        .body(axum::body::Body::empty())
        .expect("static 401 response")
}

/// Optional auth - extract user if present
pub async fn optional_auth(
    State(auth): State<Arc<AuthService>>,
    request: Request,
    next: Next,
) -> Response {
    let auth_header = request.headers().get("authorization");

    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Some(auth_value) = auth_header
        && let Ok(auth_str) = auth_value.to_str()
        && let Some(token) = auth_str.strip_prefix("Bearer ")
        && let Some(user) = auth.verify_token(token)
    {
        let mut request = request;
        request.extensions_mut().insert(user);
        return next.run(request).await;
    }

    next.run(request).await
}

/// Extract user from request extensions
pub fn get_user(request: &Request) -> Option<User> {
    request.extensions().get::<User>().cloned()
}

/// Glob match cho scope/package: `*` (mọi thứ), `@scope/*` (prefix theo scope), còn lại so khớp chính xác
pub fn scope_matches(pattern: &str, package: &str) -> bool {
    match pattern {
        "*" | "**" => true,
        _ => {
            if let Some(prefix) = pattern.strip_suffix("/*") {
                package == prefix || package.starts_with(&format!("{}/", prefix))
            } else {
                pattern == package
            }
        }
    }
}

/// Check if user has scope access to package
pub fn check_scope_access(user: &User, package: &str, auth: &AuthService) -> bool {
    auth.can_access(user, package)
}

/// Check if user can publish package
pub fn check_publish_access(user: &User, package: &str, auth: &AuthService) -> bool {
    auth.can_publish(user, package)
}

#[cfg(test)]
#[path = "test/auth.rs"]
mod tests;
