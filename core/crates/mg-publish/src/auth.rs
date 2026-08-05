/// Auth resolution — .npmrc → mg.toml → env (01 §3)
/// Không bao giờ log token — chỉ registry host + username (01 §8)
use anyhow::{anyhow, Result};
use base64::Engine;
use mg_config::npmrc::NpmRc;
use mg_config::registry::Registry;

#[derive(Debug, Clone, Default)]
pub struct Auth {
    pub token: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl Auth {
    pub fn is_empty(&self) -> bool {
        self.token.is_none() && self.username.is_none() && self.password.is_none()
    }

    /// Header Authorization cho reqwest (Bearer trước, Basic sau).
    pub fn header_value(&self) -> Option<String> {
        if let Some(token) = &self.token {
            Some(format!("Bearer {}", token))
        } else if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", user, pass));
            Some(format!("Basic {}", encoded))
        } else {
            None
        }
    }
}

/// Độ ưu tiên (cao → thấp): --token flag → env MG_NPM_TOKEN/NPM_TOKEN →
/// .npmrc authToken/basic → mg.toml [registry] token → basic từ npmrc.
/// Basic auth chỉ dùng khi không có token — npm yêu cầu Basic cho adduser.
pub fn resolve_auth(
    npmrc: &NpmRc,
    registry_url: &str,
    config_registry: Option<&Registry>,
    cli_token: Option<&str>,
) -> Result<Auth> {
    if let Some(t) = cli_token {
        return Ok(Auth {
            token: Some(t.to_string()),
            ..Default::default()
        });
    }

    if let Ok(t) = std::env::var("MG_NPM_TOKEN") {
        if !t.is_empty() {
            return Ok(Auth {
                token: Some(t),
                ..Default::default()
            });
        }
    }
    if let Ok(t) = std::env::var("NPM_TOKEN") {
        if !t.is_empty() {
            return Ok(Auth {
                token: Some(t),
                ..Default::default()
            });
        }
    }

    // host từ URL (bỏ scheme + path)
    let host = url::Url::parse(registry_url)
        .map(|u| u.host_str().unwrap_or_default().to_string())
        .unwrap_or_default();
    if let Some(token) = npmrc.token_for(&host) {
        return Ok(Auth {
            token: Some(token.clone()),
            ..Default::default()
        });
    }
    if let Some((user, pass)) = npmrc.basic_auth.get(&host) {
        return Ok(Auth {
            username: Some(user.clone()),
            password: Some(pass.clone()),
            ..Default::default()
        });
    }

    if let Some(reg) = config_registry {
        // auth_type ràng buộc phương thức lấy từ mg.toml: "basic" → không dùng token config
        let force_basic = reg.auth_type.as_deref() == Some("basic");
        let force_token = reg.auth_type.as_deref() == Some("token");
        if !force_basic {
            if let Some(token) = &reg.token {
                return Ok(Auth {
                    token: Some(token.clone()),
                    ..Default::default()
                });
            }
        }
        if !force_token {
            if let (Some(user), Some(pass)) = (&reg.username, &reg.password) {
                return Ok(Auth {
                    username: Some(user.clone()),
                    password: Some(pass.clone()),
                    ..Default::default()
                });
            }
        }
        if force_token && reg.token.is_none() {
            return Err(anyhow!(
                "registry {} declares auth_type = \"token\" but mg.toml [registry] has no token",
                registry_url
            ));
        }
        if force_basic && (reg.username.is_none() || reg.password.is_none()) {
            return Err(anyhow!(
                "registry {} declares auth_type = \"basic\" but mg.toml has no [registry] username/password",
                registry_url
            ));
        }
    }

    Err(anyhow!(
        "Auth not found for registry {} — set MG_NPM_TOKEN or NPM_TOKEN, .npmrc, or mg.toml [registry]",
        registry_url
    ))
}
