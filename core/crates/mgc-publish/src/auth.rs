/// Auth resolution — .npmrc → mgc.toml → env (01 §3)
/// Không bao giờ log token — chỉ registry host + username (01 §8)
use anyhow::{anyhow, Result};
use base64::Engine;
use mgc_config::npmrc::NpmRc;
use mgc_config::registry::Registry;

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

/// Độ ưu tiên (cao → thấp): --token flag → env MGC_NPM_TOKEN/NPM_TOKEN →
/// .npmrc authToken/basic → mgc.toml [registry] token → basic từ npmrc.
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

    if let Ok(t) = std::env::var("MGC_NPM_TOKEN") {
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

    // host từ URL (bỏ scheme + path) — thử cả origin + dạng //host:port (npmrc
    // chuẩn viết "//127.0.0.1:4315/:_authToken", có thể viết url-key đầy đủ)
    let parsed = url::Url::parse(registry_url).ok();
    let host = parsed
        .as_ref()
        .map(|u| u.host_str().unwrap_or_default().to_string())
        .unwrap_or_default();
    let candidates: Vec<String> = {
        let mut v = Vec::new();
        if let Some(u) = &parsed {
            let hwp = u
                .port()
                .map(|p| format!("{}:{}", host, p))
                .unwrap_or_else(|| host.clone());
            let origin = u.as_str().trim_end_matches('/').to_string();
            // ưu tiên key có port/origin trước host-thuần (tránh nhầm token
            // registry khác port cùng host, vd "//127.0.0.1" vs "//127.0.0.1:4315")
            v.push(origin);
            v.push(format!("//{hwp}"));
            v.push(hwp);
            v.push(host.clone());
        } else {
            v.push(host.clone());
        }
        v
    };
    if let Some(token) = candidates.iter().find_map(|c| npmrc.token_for(c).cloned()) {
        return Ok(Auth {
            token: Some(token),
            ..Default::default()
        });
    }
    if let Some((user, pass)) = candidates
        .iter()
        .find_map(|c| npmrc.basic_auth.get(c.as_str()).cloned())
    {
        return Ok(Auth {
            username: Some(user),
            password: Some(pass),
            ..Default::default()
        });
    }

    if let Some(reg) = config_registry {
        // auth_type ràng buộc phương thức lấy từ mgc.toml: "basic" → không dùng token config
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
                "registry {} declares auth_type = \"token\" but mgc.toml [registry] has no token",
                registry_url
            ));
        }
        if force_basic && (reg.username.is_none() || reg.password.is_none()) {
            return Err(anyhow!(
                "registry {} declares auth_type = \"basic\" but mgc.toml has no [registry] username/password",
                registry_url
            ));
        }
    }

    Err(anyhow!(
        "Auth not found for registry {} — set MGC_NPM_TOKEN or NPM_TOKEN, .npmrc, or mgc.toml [registry]",
        registry_url
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mgc_config::npmrc::NpmRc;

    #[test]
    fn token_from_url_keyed_npmrc() {
        // .npmrc chuẩn npm: //host/:_authToken — nhưng cũng chấp url-key
        let npmrc = NpmRc::parse("//127.0.0.1:4315/:_authToken=uabc\n").unwrap();
        let auth = resolve_auth(&npmrc, "http://127.0.0.1:4315", None, None).unwrap();
        assert_eq!(auth.token.as_deref(), Some("uabc"));

        let npmrc = NpmRc::parse("http://127.0.0.1:4315/:_authToken=habc\n").unwrap();
        let auth = resolve_auth(&npmrc, "http://127.0.0.1:4315", None, None).unwrap();
        assert_eq!(auth.token.as_deref(), Some("habc"));
    }

    #[test]
    fn basic_from_url_keyed_npmrc() {
        let npmrc = NpmRc::parse(
            "http://127.0.0.1:4315/:username=u\nhttp://127.0.0.1:4315/:_password=cGFzcw==\n",
        )
        .unwrap();
        let auth = resolve_auth(&npmrc, "http://127.0.0.1:4315", None, None).unwrap();
        assert_eq!(auth.username.as_deref(), Some("u"));
        // _password giữ base64 (npm format — server decode)
        assert_eq!(auth.password.as_deref(), Some("cGFzcw=="));
    }
}
