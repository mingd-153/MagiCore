//! Authentication and authorization for registry server
//! (Auth: Bearer token, Basic auth, scope-based access control — users persist qua storage)

use anyhow::Result;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use base64::Engine;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::storage::RegistryStore;

/// Authentication service — users persist trong SQLite, cache Mutex cho đọc nhanh
pub struct AuthService {
    pub admin_token: Option<String>,
    users: Mutex<HashMap<String, User>>, // token -> user (cache; source of truth = DB)
    pub scopes: HashMap<String, Vec<String>>, // scope -> allowed packages
    store: Arc<RegistryStore>,
}

impl Clone for AuthService {
    fn clone(&self) -> Self {
        Self {
            admin_token: self.admin_token.clone(),
            users: Mutex::new(self.users.lock().unwrap().clone()),
            scopes: self.scopes.clone(),
            store: self.store.clone(),
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
        }
    }

    /// Nạp user từ DB vào cache — gọi lúc khởi động (10-task-plan: users sống qua restart)
    pub async fn load_from_db(&self) -> Result<()> {
        let rows = self.store.load_users().await?;
        let mut users = self.users.lock().unwrap();
        users.clear();
        for (token, user) in rows {
            users.insert(token, user);
        }
        Ok(())
    }

    /// Add a user with token — ghi cả DB (persist) + cache
    pub fn add_user(&self, token: String, user: User) {
        self.users
            .lock()
            .unwrap()
            .insert(token.clone(), user.clone());
        // fire-and-forget async — lỗi DB không chặn adduser (ghi log)
        let store = self.store.clone();
        tokio::spawn(async move {
            if let Err(e) = store.upsert_user(&token, &user).await {
                eprintln!("[auth] persist user {} failed: {e}", user.name);
            }
        });
    }

    /// Delete user — DB + cache
    pub async fn remove_user(&self, name: &str) -> Result<bool> {
        let removed = self.store.delete_user_by_name(name).await?;
        if removed {
            self.users.lock().unwrap().retain(|_, u| u.name != name);
        }
        Ok(removed)
    }

    /// Xác thực username + password (Basic auth)
    pub fn verify_password(&self, username: &str, password: &str) -> Option<User> {
        self.users
            .lock()
            .unwrap()
            .values()
            // ponytail: scan tuyến tính, đủ cho registry private; index theo name khi scale
            .find(|u| u.name == username && u.password.as_deref() == Some(password))
            .cloned()
    }

    /// Verify token and return user (owned)
    pub fn verify_token(&self, token: &str) -> Option<User> {
        if let Some(admin) = &self.admin_token {
            if token == admin {
                return Some(User {
                    name: "admin".to_string(),
                    is_admin: true,
                    role: UserRole::Admin,
                    scopes: vec![],
                    password: None,
                    email: None,
                });
            }
        }
        self.users.lock().unwrap().get(token).cloned()
    }

    /// Check if user can access package (scope-based, glob: `@scope/*`, `*`)
    pub fn can_access(&self, user: &User, package: &str) -> bool {
        if user.is_admin {
            return true;
        }
        // User scope patterns trực tiếp (vd: "@megagate/*")
        for scope in &user.scopes {
            if scope_matches(scope, package) {
                return true;
            }
        }
        // Check scope mapping
        for scope in &user.scopes {
            if let Some(packages) = self.scopes.get(scope) {
                if packages.iter().any(|p| scope_matches(p, package)) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if user can publish package (task #4: viewer = read-only)
    pub fn can_publish(&self, user: &User, package: &str) -> bool {
        user.role.can_publish() && self.can_access(user, package)
    }
}

/// User representation
#[derive(Debug, Clone)]
pub struct User {
    pub name: String,
    pub is_admin: bool,
    pub role: UserRole,
    pub scopes: Vec<String>,
    pub password: Option<String>,
    pub email: Option<String>,
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

    pub fn from_str(s: &str) -> Self {
        match s {
            "publisher" => UserRole::Publisher,
            "admin" => UserRole::Admin,
            _ => UserRole::Viewer,
        }
    }

    pub fn can_publish(&self) -> bool {
        matches!(self, UserRole::Publisher | UserRole::Admin)
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
        if let Some(encoded) = auth_str.strip_prefix("Basic ") {
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded) {
                let decoded_str = String::from_utf8(decoded).ok()?;
                let mut parts = decoded_str.splitn(2, ':');
                if let (Some(user), Some(pass)) = (parts.next(), parts.next()) {
                    return Some((user.to_string(), pass.to_string()));
                }
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

    let auth_header = request.headers().get("authorization");

    if let Some(auth_value) = auth_header {
        let auth_str = match auth_value.to_str() {
            Ok(s) => s,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        };

        if let Some(token) = auth_str.strip_prefix("Bearer ") {
            if let Some(user) = auth.verify_token(token) {
                let mut request = request;
                request.extensions_mut().insert(user);
                return next.run(request).await;
            }
        } else if let Some(encoded) = auth_str.strip_prefix("Basic ") {
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded) {
                let decoded_str = match String::from_utf8(decoded) {
                    Ok(s) => s,
                    Err(_) => return StatusCode::BAD_REQUEST.into_response(),
                };
                let mut parts = decoded_str.splitn(2, ':');
                if let (Some(_user), Some(pass)) = (parts.next(), parts.next()) {
                    // pip/shared registry clients gửi Basic auth — admin token cũng chấp nhận
                    if let Some(admin) = &auth.admin_token {
                        if pass == admin {
                            let mut request = request;
                            request.extensions_mut().insert(User {
                                name: "admin".to_string(),
                                is_admin: true,
                                role: UserRole::Admin,
                                scopes: vec![],
                                password: None,
                                email: None,
                            });
                            return next.run(request).await;
                        }
                    }
                    if let Some(stored_user) = auth.verify_password(_user, pass) {
                        let mut request = request;
                        request.extensions_mut().insert(stored_user);
                        return next.run(request).await;
                    }
                }
            }
        }
    }

    unauthorized_response()
}

/// 401 kèm WWW-Authenticate challenge (chuẩn OCI/Docker).
pub fn unauthorized_response() -> Response {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(
            "WWW-Authenticate",
            "Basic realm=\"megagate-registry\", Bearer realm=\"megagate-registry\"",
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

    if let Some(auth_value) = auth_header {
        if let Ok(auth_str) = auth_value.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                if let Some(user) = auth.verify_token(token) {
                    let mut request = request;
                    request.extensions_mut().insert(user);
                    return next.run(request).await;
                }
            }
        }
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
