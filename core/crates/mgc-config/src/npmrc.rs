/// .npmrc parser — chuẩn npm, mgc tự parse (không chạy npm CLI).
/// Read npmrc and expand environment placeholders — đọc npmrc và khai triển biến môi trường.
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use std::fmt;

/// Never Debug-print tokens: counts and hosts only, values stay out of logs.
/// (Không bao giờ Debug-print token: chỉ số lượng và host.)
impl fmt::Debug for NpmRc {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NpmRc")
            .field("registry", &self.registry)
            .field("scope_registries", &self.scope_registries)
            .field(
                "auth_token_hosts",
                &self.auth_tokens.keys().collect::<Vec<_>>(),
            )
            .field(
                "basic_auth_hosts",
                &self.basic_auth.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

#[derive(Clone, Default)]
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
            // A planted symlink here would pull foreign files (e.g. another
            // user's tokens) into auth resolution — refuse, don't follow.
            // (Symlink trong project có thể kéo file ngoài vào auth.)
            if std::fs::symlink_metadata(&project_npmrc)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false)
            {
                anyhow::bail!(
                    "project .npmrc '{}' must not be a symlink",
                    project_npmrc.display()
                );
            }
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
        host.trim_start_matches('/')
            .trim_end_matches('/')
            .to_string()
    }

    /// Registry cho scoped package (@scope → URL), fallback registry mặc định.
    pub fn registry_for(&self, scope: Option<&str>) -> Option<String> {
        scope
            .and_then(|s| self.scope_registries.get(s))
            .cloned()
            .or_else(|| self.registry.clone())
    }

    /// Ghi `//host/:_authToken=TOKEN` vào file .npmrc (thay dòng cũ nếu có).
    /// (login flow — lưu token sau `mgc login` / `mgc registry user add`)
    ///
    /// Security: refuses symlinked paths (a planted link would redirect a
    /// live token outside the project) and creates the file `0600` on
    /// Unix, mirroring the keyring writer. Existing files keep their
    /// content; only the token line is replaced.
    /// (Từ chối path symlink; file mới `0600` trên Unix như keyring.)
    pub fn save_auth_token(npmrc_path: &Path, host: &str, token: &str) -> Result<()> {
        use std::fs;
        use std::io::Write;

        if fs::symlink_metadata(npmrc_path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            anyhow::bail!(
                "refusing to write auth token through symlink '{}'",
                npmrc_path.display()
            );
        }

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
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(npmrc_path)?;
        f.write_all(out.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(npmrc_path, permissions)?;
        }
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
