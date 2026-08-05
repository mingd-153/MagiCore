/// .npmrc parser — chuẩn npm, mg tự parse (không chạy npm CLI).
/// (npmrc reader — sys-mg/01 §3; env expansion ${VAR} + $VAR)
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct NpmRc {
    /// registry=URL (registry mặc định)
    pub registry: Option<String>,
    /// @scope:registry=URL — registry riêng theo scope
    pub scope_registries: HashMap<String, String>,
    /// //host/:_authToken=TOKEN
    pub auth_tokens: HashMap<String, String>,
    /// //host/:username + //host/:_password (base64)
    pub basic_auth: HashMap<String, (String, String)>,
}

impl NpmRc {
    /// Đọc .npmrc từ đường dẫn (project) + ~/.npmrc (user) — project ghi đè user.
    pub fn load(project_dir: &Path) -> Result<Self> {
        let mut combined = Self::default();
        if let Some(user) = dirs::home_dir() {
            let user_npmrc = user.join(".npmrc");
            if user_npmrc.exists() {
                combined.merge(Self::parse(&std::fs::read_to_string(&user_npmrc)?)?);
            }
        }
        let project_npmrc = project_dir.join(".npmrc");
        if project_npmrc.exists() {
            combined.merge(Self::parse(&std::fs::read_to_string(&project_npmrc)?)?);
        }
        Ok(combined)
    }

    pub fn parse(content: &str) -> Result<Self> {
        let mut rc = Self::default();
        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = expand_env(value.trim());

            if key == "registry" {
                rc.registry = Some(value);
            } else if let Some(scope) = key.strip_suffix(":registry") {
                // @scope:registry=URL
                if scope.starts_with('@') {
                    rc.scope_registries.insert(scope.to_string(), value);
                }
            } else if let Some(host) = key.strip_suffix(":_authToken") {
                let host = Self::normalize_host(host);
                rc.auth_tokens.insert(host, value);
            } else if let Some(host) = key.strip_suffix(":username") {
                let host = Self::normalize_host(host);
                let (name, _) = rc.basic_auth.entry(host).or_default();
                *name = value;
            } else if let Some(host) = key.strip_suffix(":_password") {
                let host = Self::normalize_host(host);
                let (name, pass) = rc.basic_auth.entry(host).or_default();
                if name.is_empty() {
                    // password trước username: tạm lưu, username set sau → pass giữ
                    *pass = value;
                } else {
                    *pass = value;
                }
            }
            // key khác (cache, always-auth...) — bỏ qua
        }
        Ok(rc)
    }

    pub fn merge(&mut self, other: Self) {
        if other.registry.is_some() {
            self.registry = other.registry;
        }
        for (k, v) in other.scope_registries {
            self.scope_registries.insert(k, v);
        }
        for (k, v) in other.auth_tokens {
            self.auth_tokens.insert(k, v);
        }
        for (k, v) in other.basic_auth {
            self.basic_auth.insert(k, v);
        }
    }

    /// Token cho một registry host (vd: registry.npmjs.org).
    pub fn token_for(&self, host: &str) -> Option<&String> {
        self.auth_tokens.get(&Self::normalize_host(host))
    }

    /// Normalize host key: //registry.npmjs.org/ → registry.npmjs.org
    pub fn normalize_host(host: &str) -> String {
        host.trim_start_matches('/').trim_end_matches('/').to_string()
    }

    /// Registry cho scoped package (@scope → URL), fallback registry mặc định.
    pub fn registry_for(&self, scope: Option<&str>) -> Option<String> {
        scope
            .and_then(|s| self.scope_registries.get(s))
            .cloned()
            .or_else(|| self.registry.clone())
    }

    /// Ghi `//host/:_authToken=TOKEN` vào file .npmrc (thay dòng cũ nếu có).
    /// (login flow — lưu token sau `mg login` / `mg registry user add`)
    pub fn save_auth_token(npmrc_path: &Path, host: &str, token: &str) -> Result<()> {
        use std::fs;
        use std::io::Write;

        let host_key = format!("//{}/:_authToken", Self::normalize_host(host));
        let new_line = format!("{}={}", host_key, token);

        let mut lines: Vec<String> = Vec::new();
        if npmrc_path.exists() {
            let content = fs::read_to_string(npmrc_path)?;
            let mut replaced = false;
            for raw in content.lines() {
                let line = raw.trim_end();
                if line.starts_with(&format!("{}=", host_key)) {
                    lines.push(new_line.clone());
                    replaced = true;
                } else {
                    lines.push(line.to_string());
                }
            }
            if !replaced {
                lines.push(new_line);
            }
        } else {
            lines.push(new_line);
        }

        let mut out = String::new();
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
        let mut f = fs::File::create(npmrc_path)?;
        f.write_all(out.as_bytes())?;
        Ok(())
    }
}

/// Env expansion: `${VAR}` + `$VAR` → giá trị env (nếu có; không có → giữ nguyên)
fn expand_env(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('$') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(braced) = after.strip_prefix('{') {
            // ${VAR} form
            if let Some(end) = braced.find('}') {
                out.push_str(&env_lookup(&braced[..end]));
                rest = &braced[end + 1..];
                continue;
            }
        }
        if let Some(end) = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            out.push_str(&env_lookup(&after[..end]));
            rest = &after[end..];
        } else {
            out.push_str(&env_lookup(after));
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}

fn env_lookup(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}
