/// Registry configuration
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub priority: u32,
    /// Token auth (publish) — Phase 0 fields, mg.toml [registry]
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// Ràng buộc phương thức auth: "token" | "basic" — None = auto (token trước).
    /// (mg.toml [registry] — chỉ ảnh hưởng auth lấy từ config, không đè npmrc/env)
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
