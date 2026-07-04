use std::path::Path;

pub fn redact_auth(value: &str) -> String {
    if value.len() > 8 {
        format!("{}****", &value[..4])
    } else {
        "****".to_string()
    }
}

pub fn check_auth_security(project_dir: &Path) -> Vec<String> {
    let mut warnings = Vec::new();

    let project_npmrc = project_dir.join(".npmrc");
    if project_npmrc.exists() {
        if let Ok(content) = std::fs::read_to_string(&project_npmrc) {
            if content.contains("_authToken")
                || content.contains("_auth=")
                || content.contains("_password=")
            {
                warnings.push(format!(
                    "AUTH TOKEN FOUND IN PROJECT-LEVEL .npmrc!\n  \
                     File: {}\n  \
                     Risk: Token may be accidentally committed to git.\n  \
                     Fix: Move _authToken to ~/.npmrc or use `mg config set _authToken <token>`",
                    project_npmrc.display()
                ));
            }
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&project_npmrc) {
                let mode = meta.permissions().mode();
                if mode & 0o077 != 0 {
                    warnings.push(format!(
                        "WARNING: .npmrc has permissive permissions ({:03o})!\n  \
                         Fix: chmod 600 {}",
                        mode & 0o777,
                        project_npmrc.display()
                    ));
                }
            }
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let user_npmrc = Path::new(&home).join(".npmrc");
        if user_npmrc.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&user_npmrc) {
                    let mode = meta.permissions().mode();
                    if mode & 0o077 != 0 {
                        warnings.push(format!(
                            "WARNING: ~/.npmrc has permissive permissions ({:03o})!\n  \
                             Fix: chmod 600 ~/.npmrc",
                            mode & 0o777
                        ));
                    }
                }
            }
        }
    }

    warnings
}

pub fn check_url_for_credentials(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.password().is_some()
        || (!parsed.username().is_empty() && parsed.username() != "oauth2")
    {
        return Some(format!(
            "WARNING: Registry URL '{}' contains embedded credentials!\n  \
             Risk: Credentials may leak in logs or .npmrc.\n  \
             Fix: Use `_authToken` in .npmrc instead of embedding in URL.",
            url
        ));
    }
    None
}

pub fn check_url_for_query_token(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    for (key, _) in parsed.query_pairs() {
        let kl = key.to_lowercase();
        if kl.contains("token") || kl.contains("key") || kl.contains("auth") {
            return Some(format!(
                "WARNING: Registry URL '{}' contains '{}' parameter!\n  \
                 Risk: Credentials may leak in logs or referrer headers.\n  \
                 Fix: Use `_authToken` in .npmrc instead.",
                url, key
            ));
        }
    }
    None
}
