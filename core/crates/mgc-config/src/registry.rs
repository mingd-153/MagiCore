/// Registry configuration
use serde::{Deserialize, Serialize};
use std::fmt;

/// Never Debug-print tokens: presence flags only.
/// (Không bao giờ Debug-print token: chỉ cờ có/không.)
impl fmt::Debug for Registry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Registry")
            .field("name", &self.name)
            .field("url", &self.url)
            .field("priority", &self.priority)
            .field("token", &self.token.as_ref().map(|_| "[REDACTED]"))
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("auth_type", &self.auth_type)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Registry {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub priority: u32,
    /// Token auth (publish) — Phase 0 fields, mgc.toml [registry]
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// Ràng buộc phương thức auth: "token" | "basic" — None = auto (token trước).
    /// (mgc.toml [registry] — chỉ ảnh hưởng auth lấy từ config, không đè npmrc/env)
    #[serde(default)]
    pub auth_type: Option<String>,
}

impl Registry {
    pub fn new(name: String, url: String) -> Self {
        Self {
            name,
            url,
            priority: 0,
            token: None,
            username: None,
            password: None,
            auth_type: None,
        }
    }
}
